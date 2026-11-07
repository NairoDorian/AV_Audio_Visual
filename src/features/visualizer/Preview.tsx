// Live preview canvas (blueprint section 7). Subscribes to the frame bus only while it is on
// screen (IntersectionObserver), draws in requestAnimationFrame, caps DPR at 2 and rebuilds its
// gradient only when the size changes. Nothing here touches reactive state per frame.
import { onSettled } from "solid-js";
import type { DecodedFrame } from "../../lib/frame/wire";
import { frameBus } from "../../lib/ipc";

const PALETTE = ["#3d6bff", "#8a5cff", "#ff5ca8"];
const BAR_WIDTH = 0.7;

export default function Preview(props: { onError?: (message: string) => void }) {
  let canvas: HTMLCanvasElement | undefined;

  onSettled(() => {
    const el = canvas;
    const ctx = el?.getContext("2d");
    if (!el || !ctx) return;
    let gradient: CanvasGradient | null = null;
    let frame: DecodedFrame | null = null;
    let drawQueued = 0;
    let unsubscribe: (() => void) | null = null;

    const resize = () => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const { width, height } = el.getBoundingClientRect();
      el.width = Math.max(1, Math.round(width * dpr));
      el.height = Math.max(1, Math.round(height * dpr));
      gradient = ctx.createLinearGradient(0, el.height, 0, 0);
      PALETTE.forEach((c, i) => gradient!.addColorStop(i / (PALETTE.length - 1), c));
      draw();
    };

    const draw = () => {
      drawQueued = 0;
      const { width: w, height: h } = el;
      ctx.clearRect(0, 0, w, h);
      if (!frame || frame.bars.length === 0) return;
      const bars = frame.bars;
      const slot = w / bars.length;
      const bw = Math.max(1, slot * BAR_WIDTH);
      const radius = Math.min(bw / 2, 6);
      ctx.fillStyle = gradient ?? PALETTE[0]!;
      ctx.beginPath();
      for (let i = 0; i < bars.length; i++) {
        const bh = Math.max(0, Math.min(1, bars[i]!)) * h;
        if (bh < 0.5) continue;
        ctx.roundRect(i * slot + (slot - bw) / 2, h - bh, bw, bh, [radius, radius, 0, 0]);
      }
      ctx.fill();
    };

    const subscribe = () => {
      unsubscribe ??= frameBus.subscribe({
        frame: (f) => {
          frame = f;
          drawQueued ||= requestAnimationFrame(draw);
        },
        error: (e) => props.onError?.(e.message),
      });
    };
    const unsubscribeNow = () => {
      unsubscribe?.();
      unsubscribe = null;
    };

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(el);
    const visibility = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) subscribe();
      else unsubscribeNow();
    });
    visibility.observe(el);

    // Solid 2: the settled callback returns its own cleanup (runs on owner disposal).
    return () => {
      resizeObserver.disconnect();
      visibility.disconnect();
      unsubscribeNow();
      if (drawQueued) cancelAnimationFrame(drawQueued);
    };
  });

  return <canvas ref={(e) => (canvas = e)} class="preview" aria-label="Live spectrum preview" />;
}
