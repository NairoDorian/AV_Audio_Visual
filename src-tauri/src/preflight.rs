//! Development preflight (startup step 5): refuse to create the Studio webview unless the Vite
//! dev server really serves it.
//!
//! Provenance: adapted from CursorFX `src-tauri/src/front_end.rs` (MIT OR Apache-2.0). CursorFX
//! documents three Windows reboots caused by a debug build loading the webview error page into a
//! transparent full-desktop window. AV_AudioVisual's visualizer windows are webview-less
//! (ADR-013), so the hazard is removed at the source; the preflight still keeps a debug build
//! from opening a Studio that shows an error page, and gates any `viz-` fallback webview.
//!
//! A debug build loads its UI from `build.devUrl`; only `bun run tauri dev` starts that server.
//! A release build embeds `frontendDist`, so there is nothing to check.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, TcpStream};
use std::time::Duration;

/// Port from `build.devUrl` in `tauri.conf.json` and `server.port` in `vite.config.ts`.
pub const DEV_PORT: u16 = 1420;
/// Long enough for a slow first Vite transform, short enough to report a missing server fast.
const PROBE_TIMEOUT: Duration = Duration::from_millis(750);
/// The document the Studio loads.
const PROBE_PATH: &str = "/index.html";

/// `Ok(())` when it is safe to create webviews, otherwise the message to print before exiting.
pub fn check(is_debug_build: bool) -> Result<(), String> {
    if !is_debug_build || serves_studio(&probe_addrs(DEV_PORT)) {
        return Ok(());
    }
    Err(refusal_message())
}

/// Both loopback families, IPv6 first: Vite binds to whatever `localhost` resolves to, which is
/// `[::1]` on most systems, so an IPv4-only probe would refuse a good start.
fn probe_addrs(port: u16) -> Vec<SocketAddr> {
    vec![
        SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0)),
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
    ]
}

/// A real `GET`, not a bare connect: a connect-only probe passes on a port where something else
/// listens. The status line is the whole test.
fn serves_studio(addrs: &[SocketAddr]) -> bool {
    addrs.iter().any(|addr| {
        let Ok(mut stream) = TcpStream::connect_timeout(addr, PROBE_TIMEOUT) else {
            return false;
        };
        if stream.set_read_timeout(Some(PROBE_TIMEOUT)).is_err() {
            return false;
        }
        let request = format!(
            "GET {PROBE_PATH} HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\nAccept: */*\r\n\r\n",
            addr.port()
        );
        if stream.write_all(request.as_bytes()).is_err() {
            return false;
        }
        let mut buf = [0u8; 256];
        match stream.read(&mut buf) {
            Ok(n) if n > 0 => http_is_success(&String::from_utf8_lossy(&buf[..n])),
            _ => false,
        }
    })
}

fn http_is_success(head: &str) -> bool {
    head.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .is_some_and(|code| (200..300).contains(&code))
}

fn refusal_message() -> String {
    format!(
        "refusing to start: this is a debug build, so the Studio front end is NOT bundled into it \
         and the Vite dev server is not answering on http://localhost:{DEV_PORT}{PROBE_PATH}.\n\
         Start the app with:\n    bun run tauri dev\n\
         Never run target/debug/av-app on its own. For a build that needs no server:\n    \
         bun run tauri build\n\
         Nothing was started: no window was created, so there is nothing to clean up."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(response: &'static [u8]) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
                let _ = s.write_all(response);
            }
        });
        addr
    }

    #[test]
    fn release_builds_never_probe() {
        assert!(check(false).is_ok());
    }

    #[test]
    fn a_2xx_answer_passes() {
        let addr = serve_once(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        assert!(serves_studio(&[addr]));
    }

    #[test]
    fn a_404_answer_is_refused() {
        let addr = serve_once(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        assert!(!serves_studio(&[addr]));
    }

    #[test]
    fn a_closed_port_is_refused() {
        let addr = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        assert!(!serves_studio(&[addr]));
    }

    #[test]
    fn status_line_parsing() {
        assert!(http_is_success("HTTP/1.1 204 No Content"));
        assert!(!http_is_success("HTTP/1.1 500 Oops"));
        assert!(!http_is_success("garbage"));
        assert!(!http_is_success(""));
    }

    #[test]
    fn the_message_names_the_launch_command() {
        assert!(refusal_message().contains("bun run tauri dev"));
    }
}
