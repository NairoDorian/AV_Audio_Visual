// Live preview canvas (blueprint section 7). Subscribes to the frame bus only while it is on
// screen (IntersectionObserver), draws in requestAnimationFrame, caps DPR at 2 and rebuilds its
// gradient only when the size or the scene changes. Nothing here touches reactive state per
// frame: the scene prop is read when a frame is drawn.
import { onSettled } from "solid-js";
import type { DecodedFrame } from "../../lib/frame/wire";
import { frameBus, type Scene } from "../../lib/ipc";
import { drawRing, poolRing } from "../../lib/render/concentric";
import { poolPeak } from "../../lib/render/pool";

const FALLBACK_PALETTE = ["#3d6bff", "#8a5cff", "#ff5ca8"];

export default function Preview(props: {
  scene: Scene | null;
  /** Display bars (bar styles) or ring bins (concentric); defaults to the scene's own. */
  displayCount?: number;
  onError?: (message: string) => void;
}) {
  let canvas: HTMLCanvasElement | undefined;

  onSettled(() => {
    const el = canvas;
    const ctx = el?.getContext("2d");
    if (!el || !ctx) return;
    let gradient: CanvasGradient | null = null;
    let gradientKey = "";
    let pooled = new Float32Array(96);
    let pooledBars = new Float32Array(64);
    let frame: DecodedFrame | null = null;
    let drawQueued = 0;
    let unsubscribe: (() => void) | null = null;

    /** Rebuilt only when size, style or palette change. */
    const ensureGradient = (scene: Scene | null) => {
      const palette = scene?.palette.length ? scene.palette : FALLBACK_PALETTE;
      const style = scene?.style ?? "bars";
      const radius = (scene?.ring.radius ?? 0.5) * Math.min(el.width, el.height) * 0.5;
      const key = `${el.width}x${el.height}:${style}:${radius}:${palette.join(",")}`;
      if (key === gradientKey && gradient) return gradient;
      gradientKey = key;
      if (style === "concentric") {
        const cx = el.width / 2;
        const cy = el.height / 2;
        gradient = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius * 2);
        gradient.addColorStop(0, palette[0]!);
        palette.forEach((c, i) =>
          gradient!.addColorStop(0.5 + (0.5 * i) / Math.max(1, palette.length - 1), c),
        );
      } else {
        gradient = ctx.createLinearGradient(0, el.height, 0, 0);
        palette.forEach((c, i) => gradient!.addColorStop(i / Math.max(1, palette.length - 1), c));
      }
      return gradient;
    };

    const resize = () => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const { width, height } = el.getBoundingClientRect();
      el.width = Math.max(1, Math.round(width * dpr));
      el.height = Math.max(1, Math.round(height * dpr));
      gradientKey = "";
      draw();
    };

    const drawBars = (bars: Float32Array, scene: Scene | null) => {
      const { width: w, height: h } = el;
      const slot = w / bars.length;
      const bw = Math.max(1, slot * (scene?.barWidth ?? 0.7));
      const radius = Math.min(bw / 2, 6) * (scene?.cornerRadius ?? 1);
      ctx.beginPath();
      for (let i = 0; i < bars.length; i++) {
        const bh = Math.max(0, Math.min(1, bars[i]!)) * h;
        if (bh < 0.5) continue;
        ctx.roundRect(i * slot + (slot - bw) / 2, h - bh, bw, bh, [radius, radius, 0, 0]);
      }
      ctx.fill();
    };

    const draw = () => {
      drawQueued = 0;
      const scene = props.scene;
      ctx.clearRect(0, 0, el.width, el.height);
      if (!frame || frame.bars.length === 0) return;
      const fill = ensureGradient(scene);
      ctx.globalAlpha = scene?.opacity ?? 1;
      ctx.fillStyle = fill;
      if (scene?.style === "concentric") {
        const ring = { ...scene.ring, bins: props.displayCount ?? scene.ring.bins };
        if (pooled.length !== ring.bins) pooled = new Float32Array(ring.bins);
        poolRing(frame.bars, ring, pooled);
        const r = ring.radius * Math.min(el.width, el.height) * 0.5;
        drawRing(ctx, pooled, el.width / 2, el.height / 2, r, {
          bars: ring.bars,
          inner: ring.inner,
          fill,
          base: scene.palette[0] ?? FALLBACK_PALETTE[0]!,
        });
      } else {
        const n = Math.max(1, props.displayCount ?? scene?.barCount ?? 64);
        if (n >= frame.bars.length) {
          drawBars(frame.bars, scene);
        } else {
          if (pooledBars.length !== n) pooledBars = new Float32Array(n);
          drawBars(poolPeak(frame.bars, pooledBars), scene);
        }
      }
      ctx.globalAlpha = 1;
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
