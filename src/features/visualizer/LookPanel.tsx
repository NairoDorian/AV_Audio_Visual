// The concentric components and the raw-audio scope (docs/super-fft.md sections 3 and 4), as
// controls over the preview's copy of the active scene. Preview-only until user scene files
// exist (Phase 3): nothing here is persisted.
import { For, Show } from "solid-js";
import type { Scene, SceneColors } from "../../lib/ipc";

type RingToggle = "outerLine" | "innerLine" | "outerBars" | "innerBars" | "spanBars" | "baseCircle";
type ScopeToggle = "enabled" | "horizontalMirror" | "verticalMirror";

const RING_TOGGLES: ReadonlyArray<[RingToggle, string]> = [
  ["outerLine", "Outer line"],
  ["innerLine", "Inner line"],
  ["outerBars", "Outer bars"],
  ["innerBars", "Inner bars"],
  ["spanBars", "Spanning bars"],
  ["baseCircle", "Base circle"],
];

type ColorKey = keyof SceneColors;

/** Which colours apply to a style. */
const RING_COLORS: ReadonlyArray<[ColorKey, string]> = [
  ["outerLine", "Outer line"],
  ["innerLine", "Inner line"],
  ["outerBars", "Outer bars"],
  ["innerBars", "Inner bars"],
  ["spanBars", "Spanning bars"],
  ["baseCircle", "Base circle"],
  ["scope", "Raw audio"],
];
const BAR_COLORS: ReadonlyArray<[ColorKey, string]> = [
  ["bars", "Bars"],
  ["scope", "Raw audio"],
];

/** `#rrggbb` for the colour input (drops an alpha byte). */
const rgb = (c: string | null, fallback: string) => (c ?? fallback).slice(0, 7);

const SCOPE_TOGGLES: ReadonlyArray<[ScopeToggle, string]> = [
  ["enabled", "Raw audio scope"],
  ["horizontalMirror", "Horizontal mirror (right to left copy)"],
  ["verticalMirror", "Vertical mirror (inverted copy)"],
];

/** Edits arrive as updaters over the latest scene, so two edits in one tick both land. */
export default function LookPanel(props: {
  scene: Scene;
  onUpdate: (update: (scene: Scene) => Scene) => void;
}) {
  const ring = (patch: Partial<Scene["ring"]>) =>
    props.onUpdate((s) => ({ ...s, ring: { ...s.ring, ...patch } }));
  const scope = (patch: Partial<Scene["scope"]>) =>
    props.onUpdate((s) => ({ ...s, scope: { ...s.scope, ...patch } }));
  const concentric = () => props.scene.style === "concentric";
  const color = (key: ColorKey, value: string | null) =>
    props.onUpdate((s) => ({ ...s, colors: { ...s.colors, [key]: value } }));
  const fallback = (key: ColorKey) =>
    key === "baseCircle"
      ? (props.scene.palette[0] ?? "#3d6bff")
      : key === "scope"
        ? (props.scene.palette.at(-1) ?? "#ff5ca8")
        : (props.scene.palette[1] ?? props.scene.palette[0] ?? "#8a5cff");

  return (
    <section class="card form look">
      <Show when={concentric()}>
        <div class="toggles">
          <For each={RING_TOGGLES}>
            {([key, label]) => (
              <label class="check">
                <input
                  type="checkbox"
                  checked={props.scene.ring[key]}
                  onChange={(e) => ring({ [key]: e.currentTarget.checked })}
                />
                <span>{label}</span>
              </label>
            )}
          </For>
        </div>
        <label>
          <span>Bar scale</span>
          <input
            type="range"
            min="0.5"
            max="1"
            step="0.01"
            value={props.scene.ring.barScale}
            onInput={(e) => ring({ barScale: +e.currentTarget.value })}
          />
          <output>{props.scene.ring.barScale.toFixed(2)}</output>
        </label>
        <label>
          <span>Ring floor</span>
          <input
            type="range"
            min="0"
            max="0.9"
            step="0.01"
            value={props.scene.ring.floor}
            onInput={(e) => ring({ floor: +e.currentTarget.value })}
          />
          <output>{props.scene.ring.floor.toFixed(2)}</output>
        </label>
        <label>
          <span>Ring gain</span>
          <input
            type="range"
            min="0.05"
            max="8"
            step="0.05"
            value={props.scene.ring.gain}
            onInput={(e) => ring({ gain: +e.currentTarget.value })}
          />
          <output>{props.scene.ring.gain.toFixed(2)}</output>
        </label>
      </Show>
      <div class="toggles">
        <For each={SCOPE_TOGGLES}>
          {([key, label]) => (
            <label class="check">
              <input
                type="checkbox"
                checked={props.scene.scope[key]}
                onChange={(e) => scope({ [key]: e.currentTarget.checked })}
              />
              <span>{label}</span>
            </label>
          )}
        </For>
      </div>
      <Show when={concentric()}>
        <label>
          <span>Rotational copies</span>
          <input
            type="range"
            min="1"
            max="8"
            step="1"
            value={props.scene.scope.copies}
            onInput={(e) => scope({ copies: +e.currentTarget.value })}
          />
          <output>{props.scene.scope.copies}</output>
        </label>
      </Show>
      <label>
        <span>Scope gain</span>
        <input
          type="range"
          min="0.1"
          max="16"
          step="0.1"
          value={props.scene.scope.gain}
          onInput={(e) => scope({ gain: +e.currentTarget.value })}
        />
        <output>{props.scene.scope.gain.toFixed(1)}</output>
      </label>
      <label>
        <span>Scope size</span>
        <input
          type="range"
          min="0.1"
          max="1"
          step="0.01"
          value={props.scene.scope.size}
          onInput={(e) => scope({ size: +e.currentTarget.value })}
        />
        <output>{props.scene.scope.size.toFixed(2)}</output>
      </label>
      <div class="colors">
        <For each={concentric() ? RING_COLORS : BAR_COLORS}>
          {([key, label]) => (
            <div class="color-row">
              <label class="check">
                <input
                  type="checkbox"
                  checked={props.scene.colors[key] !== null}
                  onChange={(e) =>
                    color(key, e.currentTarget.checked ? rgb(null, fallback(key)) : null)
                  }
                />
                <span>{label}</span>
              </label>
              <input
                type="color"
                aria-label={`${label} colour`}
                disabled={props.scene.colors[key] === null}
                value={rgb(props.scene.colors[key], fallback(key))}
                onInput={(e) => color(key, e.currentTarget.value)}
              />
              <span class="muted small">{props.scene.colors[key] ?? "palette"}</span>
            </div>
          )}
        </For>
      </div>
      <p class="muted small">
        Preview only until scene files: traces = 1 x 2 (horizontal mirror) x 2 (vertical mirror) x
        copies, copy i rotated by i x 180 / copies degrees.
      </p>
    </section>
  );
}
