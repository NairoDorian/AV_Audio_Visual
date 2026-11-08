// Info-bubble text of the Displays page.
import type { Help } from "../../components/InfoTip";

export const DISPLAY_HELP: Readonly<Record<string, Help>> = {
  background: {
    text: "Draws the visual on your desktop wallpaper, behind the icons. It never takes the mouse: icons stay clickable and the desktop works as usual.",
    effect: "Visible whenever the desktop is (other windows cover it as they cover the wallpaper).",
    tip: "A lower frame rate (30) is plenty for an ambient wallpaper and saves power.",
  },
  taskbar: {
    text: "A thin strip drawn over a zone of the taskbar, above the bar and click-through, so the buttons under it still work.",
    effect: "Follows the bar's edge (bottom, top or sides); paused when the bar auto-hides.",
    tip: "On Windows 11 the left of the bar is usually empty (icons are centred): the Start zone fits there.",
  },
  overlay: {
    text: "A floating window above other windows (and borderless-fullscreen games). Click-through by default, so it never steals clicks or focus.",
    effect:
      "Stays below the Start menu, notifications and exclusive-fullscreen games (Windows does not allow more without a signed UI-access build).",
  },
  enabled: {
    text: "Shows or hides this display. The window appears only after its first frame is drawn.",
  },
  monitor: {
    text: "Which screen it goes on. Primary follows whichever screen Windows calls primary; a named screen is remembered by its device identity, not its position.",
  },
  scene: {
    text: "The look this display draws. 'Active scene' follows the scene picked on the Visualizer page; a fixed scene keeps this display independent.",
    tip: "Bars suit the thin taskbar strip; the ring suits the wallpaper and the overlay.",
  },
  fps: {
    text: "The most frames per second this display draws. The analysis runs at its own rate; this only paces the drawing.",
    tip: "Match your screen's refresh rate for the smoothest motion, lower it to save power.",
  },
  scale: {
    text: "Size of the wallpaper visual as a fraction of the screen, centred. 100% fills the screen.",
  },
  zone: {
    text: "Which part of the taskbar the strip covers: the start (left), the centre, the end (next to the clock), or a custom position.",
  },
  length: {
    text: "How much of the taskbar's length the strip covers.",
  },
  offset: {
    text: "Where the strip starts, as a fraction of the taskbar's length (custom zone).",
  },
  margin: {
    text: "Space between the strip and the taskbar's edges, so it does not touch the buttons' hover highlight.",
  },
  anchor: {
    text: "Which corner, edge or centre of the screen's work area the overlay is pinned to. The offset then moves it from there; it always stays on screen.",
  },
  size: {
    text: "Width and height of the overlay in display-independent pixels: the same physical size on screens with different scaling.",
  },
  offsetXY: {
    text: "Distance from the anchor point, in display-independent pixels. Negative Y moves it up.",
  },
  clickThrough: {
    text: "When on, clicks pass through the overlay to the window below. Turning it off makes the overlay catch the mouse (for moving it in a later version).",
    tip: "Keep it on for normal use.",
  },
  captureVisible: {
    text: "Whether screenshots, screen recordings and screen sharing include the overlay. Off hides it from captures while you still see it.",
  },
  state: {
    text: "Running: drawing at the shown rate. Starting: waiting for its first frame. Anything else is why it could not start (for example an auto-hiding taskbar).",
  },
};
