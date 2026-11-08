// Live preview canvas (blueprint section 7). Subscribes to the frame bus only while it is on
// screen (IntersectionObserver), draws in requestAnimationFrame, caps DPR at 2 and rebuilds its
// gradient only when the size or the scene changes. Nothing here touches reactive state per
// frame: the scene prop is read when a frame is drawn.
import { onSettled } from "solid-js";
import type { DecodedFrame } from "../../lib/frame/wire";
import { frameBus, type Scene } from "../../lib/ipc";
import { drawRing, poolRing } from "../../lib/render/concentric";
import { poolPeak } from "../../lib/render/pool";
import { drawScope } from "../../lib/render/scope";

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
    // Reused every frame: the ring parameters with the display-count override.
    const ringParams = { bins: 96, floor: 0, gain: 1 };
    const ringStyle = {
      outerLine: true,
      innerLine: true,
      outerBars: false,
      innerBars: false,
      spanBars: true,
      baseCircle: true,
      barScale: 1,
      barSpacing: 1,
      outerLineStroke: "" as string | CanvasGradient,
      innerLineStroke: "" as string | CanvasGradient,
      outerStroke: "" as string | CanvasGradient,
      innerStroke: "" as string | CanvasGradient,
      spanStroke: "" as string | CanvasGradient,
      baseStroke: "" as string | CanvasGradient,
    };
    const scopeStyle = {
      horizontalMirror: false,
      verticalMirror: false,
      copies: 1,
      stroke: "",
      lineWidth: 1,
      clipRadius: undefined as number | undefined,
    };
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
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const scope = scene?.scope;
      const wave = scope?.enabled ? frame.waveform : null;
      const traceColor = scene?.colors.scope ?? scene?.palette.at(-1) ?? FALLBACK_PALETTE[2]!;
      if (scene?.colors.bars) ctx.fillStyle = scene.colors.bars;
      if (scene?.style === "concentric") {
        const ring = scene.ring;
        ringParams.bins = props.displayCount ?? ring.bins;
        ringParams.floor = ring.floor;
        ringParams.gain = ring.gain;
        if (pooled.length !== ringParams.bins) pooled = new Float32Array(ringParams.bins);
        poolRing(frame.bars, ringParams, pooled);
        const r = ring.radius * Math.min(el.width, el.height) * 0.5;
        const cx = el.width / 2;
        const cy = el.height / 2;
        Object.assign(ringStyle, ring);
        ringStyle.barSpacing = ring.barSpacing * dpr;
        const col = scene.colors;
        ringStyle.outerLineStroke = col.outerLine ?? fill;
        ringStyle.innerLineStroke = col.innerLine ?? fill;
        ringStyle.outerStroke = col.outerBars ?? fill;
        ringStyle.innerStroke = col.innerBars ?? fill;
        ringStyle.spanStroke = col.spanBars ?? fill;
        ringStyle.baseStroke = col.baseCircle ?? scene.palette[0] ?? FALLBACK_PALETTE[0]!;
        drawRing(ctx, pooled, cx, cy, r, ringStyle);
        if (wave && scope) {
          const half = r * scope.size;
          scopeStyle.horizontalMirror = scope.horizontalMirror;
          scopeStyle.verticalMirror = scope.verticalMirror;
          scopeStyle.copies = scope.copies;
          scopeStyle.stroke = traceColor;
          scopeStyle.lineWidth = scope.lineWidth * dpr;
          scopeStyle.clipRadius = r;
          drawScope(ctx, wave, cx, cy, half, half * 0.5 * scope.gain, scopeStyle);
        }
      } else {
        const n = Math.max(1, props.displayCount ?? scene?.barCount ?? 64);
        if (n >= frame.bars.length) {
          drawBars(frame.bars, scene);
        } else {
          if (pooledBars.length !== n) pooledBars = new Float32Array(n);
          drawBars(poolPeak(frame.bars, pooledBars), scene);
        }
        if (wave && scope) {
          // Linear scope (the taskbar strip's content): left to right, centred vertically.
          const half = (el.width / 2) * scope.size;
          scopeStyle.horizontalMirror = scope.horizontalMirror;
          scopeStyle.verticalMirror = scope.verticalMirror;
          scopeStyle.copies = 1;
          scopeStyle.stroke = traceColor;
          scopeStyle.lineWidth = scope.lineWidth * dpr;
          scopeStyle.clipRadius = undefined;
          const amp = (el.height / 2) * scope.gain;
          drawScope(ctx, wave, el.width / 2, el.height / 2, half, amp, scopeStyle);
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
