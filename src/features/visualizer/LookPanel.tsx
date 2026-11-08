// The concentric components, the raw-audio scope and the colours (docs/super-fft.md sections 3
// and 4), as controls over the preview's copy of the active scene, each with an info bubble.
// Preview-only until user scene files exist (Phase 3): nothing here is persisted.
import { For, Show } from "solid-js";
import InfoTip from "../../components/InfoTip";
import type { Scene, SceneColors } from "../../lib/ipc";
import { LOOK_HELP } from "./help";

type RingToggle = "outerLine" | "innerLine" | "outerBars" | "innerBars" | "spanBars" | "baseCircle";
type ScopeToggle = "enabled" | "horizontalMirror" | "verticalMirror";
type ColorKey = keyof SceneColors;

const RING_TOGGLES: ReadonlyArray<[RingToggle, string]> = [
  ["outerLine", "Outer line"],
  ["innerLine", "Inner line"],
  ["outerBars", "Outer bars"],
  ["innerBars", "Inner bars"],
  ["spanBars", "Spanning bars"],
  ["baseCircle", "Base circle"],
];

const SCOPE_TOGGLES: ReadonlyArray<[ScopeToggle, string, string]> = [
  ["enabled", "Raw audio scope", "scopeEnabled"],
  ["horizontalMirror", "Horizontal mirror", "horizontalMirror"],
  ["verticalMirror", "Vertical mirror", "verticalMirror"],
];

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

/** A labelled slider with its info bubble and value readout. */
function Slider(props: {
  label: string;
  help: string;
  min: number;
  max: number;
  step: number;
  value: number;
  digits: number;
  onInput: (v: number) => void;
}) {
  return (
    <div class="param">
      <span class="param-label">
        <span>{props.label}</span>
        <InfoTip
          title={props.label}
          help={LOOK_HELP[props.help]!}
          facts={[`Range: ${props.min} to ${props.max}`]}
        />
      </span>
      <input
        type="range"
        aria-label={props.label}
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onInput={(e) => props.onInput(+e.currentTarget.value)}
      />
      <output class="param-value">{props.value.toFixed(props.digits)}</output>
    </div>
  );
}

/** Edits arrive as updaters over the latest scene, so two edits in one tick both land. */
export default function LookPanel(props: {
  scene: Scene;
  onUpdate: (update: (scene: Scene) => Scene) => void;
  /** The scene is a built-in with saved edits. */
  modified: boolean;
  onReset: () => void;
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
    <section class="card look">
      <Show when={concentric()}>
        <h3 class="group-title">Ring</h3>
        <div class="toggles">
          <For each={RING_TOGGLES}>
            {([key, label]) => (
              <span class="toggle">
                <label class="check">
                  <input
                    type="checkbox"
                    checked={props.scene.ring[key]}
                    onChange={(e) => ring({ [key]: e.currentTarget.checked })}
                  />
                  <span>{label}</span>
                </label>
                <InfoTip title={label} help={LOOK_HELP[key]!} />
              </span>
            )}
          </For>
        </div>
        <div class="params">
          <Slider
            label="Bar scale"
            help="barScale"
            min={0.5}
            max={1}
            step={0.01}
            digits={2}
            value={props.scene.ring.barScale}
            onInput={(v) => ring({ barScale: v })}
          />
          <Slider
            label="Ring floor"
            help="ringFloor"
            min={0}
            max={0.9}
            step={0.01}
            digits={2}
            value={props.scene.ring.floor}
            onInput={(v) => ring({ floor: v })}
          />
          <Slider
            label="Ring gain"
            help="ringGain"
            min={0.05}
            max={8}
            step={0.05}
            digits={2}
            value={props.scene.ring.gain}
            onInput={(v) => ring({ gain: v })}
          />
        </div>
      </Show>

      <h3 class="group-title">Raw audio</h3>
      <div class="toggles">
        <For each={SCOPE_TOGGLES}>
          {([key, label, help]) => (
            <span class="toggle">
              <label class="check">
                <input
                  type="checkbox"
                  checked={props.scene.scope[key]}
                  onChange={(e) => scope({ [key]: e.currentTarget.checked })}
                />
                <span>{label}</span>
              </label>
              <InfoTip title={label} help={LOOK_HELP[help]!} />
            </span>
          )}
        </For>
      </div>
      <div class="params">
        <Show when={concentric()}>
          <Slider
            label="Rotational copies"
            help="copies"
            min={1}
            max={8}
            step={1}
            digits={0}
            value={props.scene.scope.copies}
            onInput={(v) => scope({ copies: v })}
          />
        </Show>
        <Slider
          label="Scope gain"
          help="scopeGain"
          min={0.1}
          max={16}
          step={0.1}
          digits={1}
          value={props.scene.scope.gain}
          onInput={(v) => scope({ gain: v })}
        />
        <Slider
          label="Scope size"
          help="scopeSize"
          min={0.1}
          max={1}
          step={0.01}
          digits={2}
          value={props.scene.scope.size}
          onInput={(v) => scope({ size: v })}
        />
      </div>

      <h3 class="group-title">
        Colours <InfoTip title="Colours" help={LOOK_HELP["colors"]!} />
      </h3>
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
      <div class="row look-foot">
        <p class="muted small grow">
          Changes are saved to this scene automatically and shown on every display that uses it.
        </p>
        <Show when={props.modified}>
          <button type="button" onClick={props.onReset}>
            Reset look to built-in
          </button>
        </Show>
      </div>
    </section>
  );
}
