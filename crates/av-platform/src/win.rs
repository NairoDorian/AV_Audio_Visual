//! Windows hosts (blueprint 10.2): native, webview-less windows on one `av-host-win` thread
//! with its own message loop. The render thread presents into them through raw handles (DX12
//! allows presenting from another thread).
//!
//! | Host | Window |
//! | :-- | :-- |
//! | Overlay | Top-level `WS_POPUP`, `WS_EX_TOPMOST | TOOLWINDOW | NOACTIVATE | NOREDIRECTIONBITMAP`, plus `LAYERED | TRANSPARENT` when click-through; no DWM corner rounding; optional `WDA_EXCLUDEFROMCAPTURE` |
//! | Taskbar | The same, owned by `Shell_TrayWnd` so it stays above the bar; topmost re-asserted every second |
//! | Background | A `WS_CHILD | WS_DISABLED` window behind the desktop icons: on the Windows 11 24H2 layout a child of `Progman` directly below `SHELLDLL_DefView`, on the classic layout a child of the `WorkerW` behind the icons (`0x052C` asks Explorer to create it) |
//!
//! Every window is created hidden and shown only after its first frame (invariant 15). Windows
//! are registered so [`emergency_hide_all`] can hide them from a panic hook on any thread.
//!
//! The desktop layouts are undocumented; the technique is the one the reference wallpaper mods
//! use (study `references/studies/windhawk-wallpaper-engine.md`), re-implemented here.

use std::cell::RefCell;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW, ValidateRect,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
    SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETSTATE, ABM_GETTASKBARPOS, ABS_AUTOHIDE,
    APPBARDATA, SHAppBarMessage,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, w};

use crate::geometry::{Edge, Monitor, Rect, Taskbar};

/// What kind of host to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKind {
    Overlay {
        click_through: bool,
        capture_visible: bool,
    },
    Taskbar,
    Background,
}

/// Something the hosts noticed; the owner re-resolves geometry and re-creates what is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostEvent {
    /// Resolution, DPI, work area or taskbar moved.
    DisplayChanged,
    /// Explorer restarted (`TaskbarCreated`), or a background's parent disappeared.
    ExplorerRestarted,
}

/// A native host window. Only a handle: the window lives on the host thread and is destroyed
/// through [`Hosts::destroy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostWindow {
    pub hwnd: isize,
    pub kind: HostKind,
}

impl HostWindow {
    pub fn raw_window_handle(&self) -> Option<raw_window_handle::RawWindowHandle> {
        let hwnd = std::num::NonZeroIsize::new(self.hwnd)?;
        Some(raw_window_handle::RawWindowHandle::Win32(
            raw_window_handle::Win32WindowHandle::new(hwnd),
        ))
    }

    pub fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
        raw_window_handle::RawDisplayHandle::Windows(raw_window_handle::WindowsDisplayHandle::new())
    }
}

enum Cmd {
    Create(HostKind, Rect, Sender<Result<isize, String>>),
    Place(isize, Rect),
    Show(isize),
    Destroy(isize),
    Monitors(Sender<Vec<Monitor>>),
    Taskbar(Sender<Option<Taskbar>>),
    Quit,
}

/// The host thread.
pub struct Hosts {
    tx: Sender<Cmd>,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
    /// Display and Explorer events.
    pub events: Receiver<HostEvent>,
}

impl std::fmt::Debug for Hosts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hosts")
            .field("thread_id", &self.thread_id)
            .finish_non_exhaustive()
    }
}

const WM_APP_WAKE: u32 = WM_APP + 1;
const HEALTH_TIMER_MS: u32 = 1000;
const CLASS: PCWSTR = w!("AVAudioVisualHost");
/// Asks Explorer to create the WorkerW layer behind the desktop icons (undocumented).
const SPAWN_WORKERW: u32 = 0x052C;

/// Every live host window, for [`emergency_hide_all`].
static LIVE: Mutex<Vec<isize>> = Mutex::new(Vec::new());

thread_local! {
    static EVENTS: RefCell<Option<Sender<HostEvent>>> = const { RefCell::new(None) };
    static TASKBAR_CREATED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn emit(e: HostEvent) {
    EVENTS.with(|s| {
        if let Some(tx) = s.borrow().as_ref() {
            let _ = tx.send(e);
        }
    });
}

/// Hides every host window from any thread (panic hook): a transparent topmost window that
/// outlives its renderer would cover the desktop (invariant 15). Never blocks on a poisoned or
/// held lock.
pub fn emergency_hide_all() {
    let Ok(live) = LIVE.try_lock() else { return };
    for &h in live.iter() {
        // SAFETY: ShowWindow on a handle that may already be destroyed is harmless (it fails).
        unsafe {
            let _ = ShowWindow(HWND(h as *mut _), SW_HIDE);
        }
    }
}

impl Hosts {
    /// Starts the host thread (per-monitor DPI aware v2, so every rectangle is physical pixels).
    pub fn start() -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (etx, events) = mpsc::channel::<HostEvent>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("av-host-win".into())
            .spawn(move || host_thread(rx, etx, ready_tx))
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx
            .recv()
            .map_err(|_| "the host thread exited during start".to_string())??;
        Ok(Self {
            tx,
            thread_id,
            thread: Some(thread),
            events,
        })
    }

    fn send(&self, cmd: Cmd) {
        if self.tx.send(cmd).is_ok() {
            // SAFETY: posting to our own thread's queue; failure only means it already exited.
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_APP_WAKE, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Creates a hidden host window at `rect` (physical pixels, screen coordinates).
    pub fn create(&self, kind: HostKind, rect: Rect) -> Result<HostWindow, String> {
        let (tx, rx) = mpsc::channel();
        self.send(Cmd::Create(kind, rect, tx));
        let hwnd = rx.recv().map_err(|_| "host thread gone".to_string())??;
        Ok(HostWindow { hwnd, kind })
    }

    pub fn place(&self, w: &HostWindow, rect: Rect) {
        self.send(Cmd::Place(w.hwnd, rect));
    }

    pub fn show(&self, w: &HostWindow) {
        self.send(Cmd::Show(w.hwnd));
    }

    pub fn destroy(&self, w: &HostWindow) {
        self.send(Cmd::Destroy(w.hwnd));
    }

    pub fn monitors(&self) -> Vec<Monitor> {
        let (tx, rx) = mpsc::channel();
        self.send(Cmd::Monitors(tx));
        rx.recv().unwrap_or_default()
    }

    pub fn taskbar(&self) -> Option<Taskbar> {
        let (tx, rx) = mpsc::channel();
        self.send(Cmd::Taskbar(tx));
        rx.recv().ok().flatten()
    }
}

impl Drop for Hosts {
    fn drop(&mut self) {
        self.send(Cmd::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Per-window state the host thread keeps.
struct Live {
    hwnd: HWND,
    kind: HostKind,
    /// The background's parent (Progman or WorkerW) and the window it sits below.
    parent: Option<(HWND, Option<HWND>)>,
}

fn host_thread(rx: Receiver<Cmd>, events: Sender<HostEvent>, ready: Sender<Result<u32, String>>) {
    // SAFETY: plain Win32 calls on this thread's own state.
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        EVENTS.with(|s| *s.borrow_mut() = Some(events));
        TASKBAR_CREATED.with(|c| c.set(RegisterWindowMessageW(w!("TaskbarCreated"))));
        let instance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => {
                let _ = ready.send(Err(e.to_string()));
                return;
            }
        };
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
        // A hidden top-level window receives the broadcasts (TaskbarCreated, display changes)
        // that a message-only window would not.
        let listener = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS,
            w!("AV_AudioVisual host listener"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        );
        if let Err(e) = &listener {
            let _ = ready.send(Err(format!("listener window: {e}")));
            return;
        }
        SetTimer(None, 0, HEALTH_TIMER_MS, None);
        let _ = ready.send(Ok(GetCurrentThreadId()));

        let mut live: Vec<Live> = Vec::new();
        let mut msg = MSG::default();
        loop {
            let r = GetMessageW(&mut msg, None, 0, 0);
            if r.0 <= 0 {
                break;
            }
            if msg.hwnd.is_invalid() && msg.message == WM_APP_WAKE {
                let mut quit = false;
                while let Ok(cmd) = rx.try_recv() {
                    if handle(cmd, &mut live, instance.into()) {
                        quit = true;
                    }
                }
                if quit {
                    break;
                }
                continue;
            }
            if msg.hwnd.is_invalid() && msg.message == WM_TIMER {
                health(&live);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        for l in live.drain(..) {
            let _ = DestroyWindow(l.hwnd);
        }
        if let Ok(h) = listener {
            let _ = DestroyWindow(h);
        }
        LIVE.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

/// Runs one command; returns true on Quit.
unsafe fn handle(
    cmd: Cmd,
    live: &mut Vec<Live>,
    instance: windows::Win32::Foundation::HINSTANCE,
) -> bool {
    match cmd {
        Cmd::Create(kind, rect, reply) => {
            // SAFETY: on the host thread, with our registered class.
            let r = unsafe { create(kind, rect, instance) };
            let _ = reply.send(r.map(|l| {
                let h = l.hwnd.0 as isize;
                LIVE.lock().unwrap_or_else(PoisonError::into_inner).push(h);
                live.push(l);
                h
            }));
        }
        Cmd::Place(h, rect) => {
            if let Some(l) = live.iter().find(|l| l.hwnd.0 as isize == h) {
                // SAFETY: a window this thread created.
                unsafe { place(l, rect) };
            }
        }
        Cmd::Show(h) => {
            if let Some(l) = live.iter().find(|l| l.hwnd.0 as isize == h) {
                // SAFETY: as above.
                unsafe {
                    let _ = ShowWindow(l.hwnd, SW_SHOWNOACTIVATE);
                    if !matches!(l.kind, HostKind::Background) {
                        let _ = SetWindowPos(
                            l.hwnd,
                            Some(HWND_TOPMOST),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                        );
                    }
                }
            }
        }
        Cmd::Destroy(h) => {
            if let Some(i) = live.iter().position(|l| l.hwnd.0 as isize == h) {
                let l = live.remove(i);
                // SAFETY: as above; hidden first so nothing flashes (invariant 15).
                unsafe {
                    let _ = ShowWindow(l.hwnd, SW_HIDE);
                    let _ = DestroyWindow(l.hwnd);
                }
                LIVE.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .retain(|&x| x != h);
            }
        }
        // SAFETY: read-only queries.
        Cmd::Monitors(reply) => {
            let _ = reply.send(unsafe { monitors() });
        }
        Cmd::Taskbar(reply) => {
            let _ = reply.send(unsafe { taskbar() });
        }
        Cmd::Quit => return true,
    }
    false
}

unsafe fn create(
    kind: HostKind,
    rect: Rect,
    instance: windows::Win32::Foundation::HINSTANCE,
) -> Result<Live, String> {
    match kind {
        HostKind::Overlay { .. } | HostKind::Taskbar => {
            let click_through = match kind {
                HostKind::Overlay { click_through, .. } => click_through,
                _ => true,
            };
            let mut ex =
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP;
            if click_through {
                ex |= WS_EX_LAYERED | WS_EX_TRANSPARENT;
            }
            // SAFETY: Win32 calls on the host thread.
            let owner = match kind {
                HostKind::Taskbar => unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.ok(),
                _ => None,
            };
            let hwnd = unsafe {
                CreateWindowExW(
                    ex,
                    CLASS,
                    w!("AV_AudioVisual"),
                    WS_POPUP,
                    rect.x,
                    rect.y,
                    rect.w.max(1),
                    rect.h.max(1),
                    owner,
                    None,
                    Some(instance),
                    None,
                )
            }
            .map_err(|e| format!("create window: {e}"))?;
            unsafe {
                if click_through {
                    // A layered window draws nothing until its attributes are set; full opacity,
                    // the composition swapchain carries the per-pixel alpha.
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
                }
                let round: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND;
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &round as *const _ as *const _,
                    std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
                );
                if let HostKind::Overlay {
                    capture_visible: false,
                    ..
                } = kind
                {
                    let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
                }
            }
            Ok(Live {
                hwnd,
                kind,
                parent: None,
            })
        }
        HostKind::Background => {
            // SAFETY: Win32 calls on the host thread.
            let (parent, below) =
                unsafe { desktop_host() }.ok_or("no desktop window to attach to")?;
            let origin = unsafe { window_origin(parent) };
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_NOREDIRECTIONBITMAP | WS_EX_NOACTIVATE,
                    CLASS,
                    w!("AV_AudioVisual wallpaper"),
                    WS_CHILD | WS_CLIPSIBLINGS | WS_DISABLED,
                    rect.x - origin.0,
                    rect.y - origin.1,
                    rect.w.max(1),
                    rect.h.max(1),
                    Some(parent),
                    None,
                    Some(instance),
                    None,
                )
            }
            .map_err(|e| format!("create wallpaper window: {e}"))?;
            let l = Live {
                hwnd,
                kind,
                parent: Some((parent, below)),
            };
            unsafe { order_background(&l) };
            Ok(l)
        }
    }
}

unsafe fn window_origin(hwnd: HWND) -> (i32, i32) {
    let mut r = RECT::default();
    // SAFETY: a valid out pointer.
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    (r.left, r.top)
}

unsafe fn order_background(l: &Live) {
    let Some((_, below)) = l.parent else { return };
    let after = below.unwrap_or(HWND_TOP);
    // SAFETY: windows this thread created, or Explorer's (SetWindowPos tolerates a stale one).
    unsafe {
        let _ = SetWindowPos(
            l.hwnd,
            Some(after),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

unsafe fn place(l: &Live, rect: Rect) {
    let (x, y) = match l.parent {
        // SAFETY: the parent is checked by the health timer.
        Some((parent, _)) => {
            let o = unsafe { window_origin(parent) };
            (rect.x - o.0, rect.y - o.1)
        }
        None => (rect.x, rect.y),
    };
    // SAFETY: a window this thread created.
    unsafe {
        let _ = SetWindowPos(
            l.hwnd,
            None,
            x,
            y,
            rect.w.max(1),
            rect.h.max(1),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Every second: keep overlays and the taskbar strip above the bar, keep wallpapers behind the
/// icons, and report a vanished desktop host (Explorer restarted).
unsafe fn health(live: &[Live]) {
    for l in live {
        // SAFETY: windows this thread created; the parent may be gone, which IsWindow reports.
        unsafe {
            match l.parent {
                Some((parent, _)) => {
                    if !IsWindow(Some(parent)).as_bool() {
                        emit(HostEvent::ExplorerRestarted);
                    } else {
                        order_background(l);
                    }
                }
                None => {
                    if IsWindowVisible(l.hwnd).as_bool() {
                        let _ = SetWindowPos(
                            l.hwnd,
                            Some(HWND_TOPMOST),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
                }
            }
        }
    }
}

/// The window the wallpaper attaches to, and the window it must sit directly below.
unsafe fn desktop_host() -> Option<(HWND, Option<HWND>)> {
    // SAFETY: shell windows looked up by class; the message has a 1 s timeout.
    unsafe {
        let progman = FindWindowW(w!("Progman"), None).ok()?;
        SendMessageTimeoutW(
            progman,
            SPAWN_WORKERW,
            WPARAM(0xD),
            LPARAM(1),
            SMTO_NORMAL,
            1000,
            None,
        );
        // Windows 11 24H2+: Progman hosts the icon view; sit directly below it.
        if let Ok(defview) = FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), None) {
            return Some((progman, Some(defview)));
        }
        // Classic: the WorkerW that follows the top-level window owning the icon view.
        let mut found = HWND::default();
        let _ = EnumWindows(Some(find_workerw), LPARAM(&mut found as *mut HWND as isize));
        (!found.is_invalid()).then_some((found, None))
    }
}

unsafe extern "system" fn find_workerw(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the `&mut HWND` passed by `desktop_host` for the enumeration's duration.
    unsafe {
        if FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), None).is_ok()
            && let Ok(worker) = FindWindowExW(None, Some(hwnd), w!("WorkerW"), None)
        {
            *(lparam.0 as *mut HWND) = worker;
            return BOOL(0);
        }
    }
    BOOL(1)
}

unsafe fn monitors() -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    // SAFETY: the callback writes into `out`, alive for the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_proc),
            LPARAM(&mut out as *mut Vec<Monitor> as isize),
        );
    }
    out
}

unsafe extern "system" fn monitor_proc(
    hmon: HMONITOR,
    _hdc: HDC,
    _clip: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    // SAFETY: lparam is the `&mut Vec<Monitor>` from `monitors`; MONITORINFOEXW starts with
    // MONITORINFO and carries its own size.
    unsafe {
        let out = &mut *(lparam.0 as *mut Vec<Monitor>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(hmon, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            let name_len = info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szDevice.len());
            let r = info.monitorInfo.rcMonitor;
            let wa = info.monitorInfo.rcWork;
            out.push(Monitor {
                id: String::from_utf16_lossy(&info.szDevice[..name_len]),
                rect: Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top),
                work: Rect::new(wa.left, wa.top, wa.right - wa.left, wa.bottom - wa.top),
                scale: dx as f32 / 96.0,
                primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            });
        }
    }
    BOOL(1)
}

unsafe fn taskbar() -> Option<Taskbar> {
    let mut abd = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    // SAFETY: a correctly sized APPBARDATA.
    unsafe {
        if SHAppBarMessage(ABM_GETTASKBARPOS, &mut abd) == 0 {
            return None;
        }
        let state = SHAppBarMessage(ABM_GETSTATE, &mut abd) as u32;
        let r = abd.rc;
        let edge = match abd.uEdge {
            ABE_LEFT => Edge::Left,
            ABE_TOP => Edge::Top,
            ABE_RIGHT => Edge::Right,
            ABE_BOTTOM => Edge::Bottom,
            _ => Edge::Bottom,
        };
        Some(Taskbar {
            rect: Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top),
            edge,
            auto_hide: state & ABS_AUTOHIDE != 0,
        })
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: the window procedure contract.
    unsafe {
        match msg {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                emit(HostEvent::DisplayChanged);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            m if m != 0 && m == TASKBAR_CREATED.with(|c| c.get()) => {
                emit(HostEvent::ExplorerRestarted);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
