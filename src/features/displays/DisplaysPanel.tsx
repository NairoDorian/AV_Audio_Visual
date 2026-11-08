// The three native displays (wallpaper, taskbar strip, overlay): one card each, every setting
// with an info bubble, the live state of each display, and the monitors to choose from.
import { createSignal, For, onSettled, Show } from "solid-js";
import type { JSX } from "@solidjs/web";
import InfoTip from "../../components/InfoTip";
import {
  getDisplayStates,
  listMonitors,
  type AnchorPoint,
  type AppConfig,
  type DisplayState,
  type MonitorInfo,
  type Scene,
  type TaskbarZone,
} from "../../lib/ipc";
import { DISPLAY_HELP } from "./help";

const STATE_POLL_MS = 1000;

type Section = "background" | "taskbar" | "overlay";

function Row(props: { label: string; help: string; facts?: string[]; children: JSX.Element }) {
  return (
    <div class="param">
      <span class="param-label">
        <span>{props.label}</span>
        <InfoTip title={props.label} help={DISPLAY_HELP[props.help]!} facts={props.facts} />
      </span>
      {props.children}
    </div>
  );
}

function Range(props: {
  label: string;
  help: string;
  min: number;
  max: number;
  step: number;
  value: number;
  format: (v: number) => string;
  onCommit: (v: number) => void;
}) {
  const [draft, setDraft] = createSignal<number | null>(null);
  return (
    <Row
      label={props.label}
      help={props.help}
      facts={[`Range: ${props.format(props.min)} to ${props.format(props.max)}`]}
    >
      <input
        type="range"
        aria-label={props.label}
        min={props.min}
        max={props.max}
        step={props.step}
        value={draft() ?? props.value}
        onInput={(e) => setDraft(+e.currentTarget.value)}
        onChange={(e) => {
          setDraft(null);
          props.onCommit(+e.currentTarget.value);
        }}
      />
      <output class="param-value">{props.format(draft() ?? props.value)}</output>
    </Row>
  );
}

const pct = (v: number) => `${Math.round(v * 100)}%`;
const fps = (v: number) => `${v} fps`;
const dip = (v: number) => `${v} DIP`;

export default function DisplaysPanel(props: {
  config: AppConfig;
  scenes: Scene[];
  onPatch: (patch: Record<string, unknown>) => void;
  onError: (message: string) => void;
}) {
  const [monitors, setMonitors] = createSignal<MonitorInfo[]>([]);
  const [states, setStates] = createSignal<DisplayState[]>([]);

  const poll = () => getDisplayStates().then(setStates, () => {});
  onSettled(() => {
    listMonitors().then(setMonitors, (e) => props.onError(String(e)));
    void poll();
    const timer = setInterval(poll, STATE_POLL_MS);
    return () => clearInterval(timer);
  });

  const patch = (section: Section, value: Record<string, unknown>) =>
    props.onPatch({ [section]: value });
  const stateOf = (p: Section) => states().find((s) => s.placement === p);

  const sceneSelect = (section: Section, value: string) => (
    <Row label="Scene" help="scene">
      <select
        class="param-wide"
        value={value}
        onChange={(e) => patch(section, { scene: e.currentTarget.value })}
      >
        <option value="">Active scene (follows the Visualizer page)</option>
        <For each={props.scenes}>{(s) => <option value={s.id}>{s.name}</option>}</For>
      </select>
    </Row>
  );

  const monitorSelect = (value: string | null, onPick: (id: string | null) => void) => (
    <Row label="Monitor" help="monitor">
      <select
        class="param-wide"
        value={value ?? ""}
        onChange={(e) => onPick(e.currentTarget.value || null)}
      >
        <option value="">Primary monitor</option>
        <For each={monitors()}>
          {(m) => (
            <option value={m.id}>
              {m.id.replace(/^\\\\\.\\/, "")} - {m.width} x {m.height}
              {m.scale !== 1 ? ` at ${Math.round(m.scale * 100)}%` : ""}
              {m.primary ? " (primary)" : ""}
            </option>
          )}
        </For>
      </select>
    </Row>
  );

  const header = (section: Section, title: string, enabled: boolean) => {
    const st = () => stateOf(section);
    return (
      <div class="display-head">
        <label class="check switch">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => patch(section, { enabled: e.currentTarget.checked })}
          />
          <h2>{title}</h2>
        </label>
        <InfoTip title={title} help={DISPLAY_HELP[section]!} />
        <Show when={enabled}>
          <span
            class={[
              "display-state",
              {
                ok: st()?.state === "running",
                bad: !!st() && !/^(running|starting)$/.test(st()!.state),
              },
            ]}
          >
            {st()
              ? st()!.state === "running"
                ? `Running, ${st()!.fps.toFixed(0)} fps, ${st()!.rect[2]} x ${st()!.rect[3]} px`
                : st()!.state
              : "Starting..."}
          </span>
          <InfoTip title="State" help={DISPLAY_HELP["state"]!} />
        </Show>
      </div>
    );
  };

  const anchors: ReadonlyArray<[AnchorPoint, string]> = [
    ["topLeft", "Top left"],
    ["topCenter", "Top centre"],
    ["topRight", "Top right"],
    ["centerLeft", "Centre left"],
    ["center", "Centre"],
    ["centerRight", "Centre right"],
    ["bottomLeft", "Bottom left"],
    ["bottomCenter", "Bottom centre"],
    ["bottomRight", "Bottom right"],
  ];
  const zones: ReadonlyArray<[TaskbarZone, string]> = [
    ["start", "Start (left)"],
    ["center", "Centre"],
    ["end", "End (by the clock)"],
    ["custom", "Custom position"],
  ];

  return (
    <>
      <section class="card display-card">
        {header("background", "Wallpaper", props.config.background.enabled)}
        <div class={["params", { inactive: !props.config.background.enabled }]}>
          {monitorSelect(props.config.background.monitor, (id) =>
            patch("background", { monitor: id }),
          )}
          {sceneSelect("background", props.config.background.scene)}
          <Range
            label="Size"
            help="scale"
            min={0.1}
            max={1}
            step={0.01}
            value={props.config.background.scale}
            format={pct}
            onCommit={(v) => patch("background", { scale: v })}
          />
          <Range
            label="Frame rate"
            help="fps"
            min={5}
            max={240}
            step={1}
            value={props.config.background.fps}
            format={fps}
            onCommit={(v) => patch("background", { fps: v })}
          />
        </div>
      </section>

      <section class="card display-card">
        {header("taskbar", "Taskbar strip", props.config.taskbar.enabled)}
        <div class={["params", { inactive: !props.config.taskbar.enabled }]}>
          <Row label="Zone" help="zone">
            <select
              class="param-wide"
              value={props.config.taskbar.zone}
              onChange={(e) => patch("taskbar", { zone: e.currentTarget.value })}
            >
              <For each={zones}>{([v, l]) => <option value={v}>{l}</option>}</For>
            </select>
          </Row>
          <Range
            label="Length"
            help="length"
            min={0.02}
            max={1}
            step={0.01}
            value={props.config.taskbar.length}
            format={pct}
            onCommit={(v) => patch("taskbar", { length: v })}
          />
          <Show when={props.config.taskbar.zone === "custom"}>
            <Range
              label="Position"
              help="offset"
              min={0}
              max={1}
              step={0.01}
              value={props.config.taskbar.offset}
              format={pct}
              onCommit={(v) => patch("taskbar", { offset: v })}
            />
          </Show>
          <Range
            label="Margin"
            help="margin"
            min={0}
            max={16}
            step={1}
            value={props.config.taskbar.marginDip}
            format={dip}
            onCommit={(v) => patch("taskbar", { marginDip: v })}
          />
          {sceneSelect("taskbar", props.config.taskbar.scene)}
          <Range
            label="Frame rate"
            help="fps"
            min={10}
            max={240}
            step={1}
            value={props.config.taskbar.fps}
            format={fps}
            onCommit={(v) => patch("taskbar", { fps: v })}
          />
        </div>
      </section>

      <section class="card display-card">
        {header("overlay", "Overlay", props.config.overlay.enabled)}
        <div class={["params", { inactive: !props.config.overlay.enabled }]}>
          {monitorSelect(props.config.overlay.anchor.monitor, (id) =>
            patch("overlay", { anchor: { monitor: id } }),
          )}
          <Row label="Position" help="anchor">
            <select
              class="param-wide"
              value={props.config.overlay.anchor.anchor}
              onChange={(e) => patch("overlay", { anchor: { anchor: e.currentTarget.value } })}
            >
              <For each={anchors}>{([v, l]) => <option value={v}>{l}</option>}</For>
            </select>
          </Row>
          <Range
            label="Width"
            help="size"
            min={48}
            max={3840}
            step={8}
            value={props.config.overlay.anchor.sizeDip[0]}
            format={dip}
            onCommit={(v) =>
              patch("overlay", { anchor: { sizeDip: [v, props.config.overlay.anchor.sizeDip[1]] } })
            }
          />
          <Range
            label="Height"
            help="size"
            min={16}
            max={2160}
            step={8}
            value={props.config.overlay.anchor.sizeDip[1]}
            format={dip}
            onCommit={(v) =>
              patch("overlay", { anchor: { sizeDip: [props.config.overlay.anchor.sizeDip[0], v] } })
            }
          />
          <Range
            label="Offset X"
            help="offsetXY"
            min={-1920}
            max={1920}
            step={4}
            value={props.config.overlay.anchor.offsetDip[0]}
            format={dip}
            onCommit={(v) =>
              patch("overlay", {
                anchor: { offsetDip: [v, props.config.overlay.anchor.offsetDip[1]] },
              })
            }
          />
          <Range
            label="Offset Y"
            help="offsetXY"
            min={-1080}
            max={1080}
            step={4}
            value={props.config.overlay.anchor.offsetDip[1]}
            format={dip}
            onCommit={(v) =>
              patch("overlay", {
                anchor: { offsetDip: [props.config.overlay.anchor.offsetDip[0], v] },
              })
            }
          />
          <Row label="Click-through" help="clickThrough">
            <input
              type="checkbox"
              checked={props.config.overlay.input === "locked"}
              onChange={(e) =>
                patch("overlay", { input: e.currentTarget.checked ? "locked" : "edit" })
              }
            />
          </Row>
          <Row label="Visible to screen capture" help="captureVisible">
            <input
              type="checkbox"
              checked={props.config.overlay.captureVisible}
              onChange={(e) => patch("overlay", { captureVisible: e.currentTarget.checked })}
            />
          </Row>
          {sceneSelect("overlay", props.config.overlay.scene)}
          <Range
            label="Frame rate"
            help="fps"
            min={10}
            max={240}
            step={1}
            value={props.config.overlay.fps}
            format={fps}
            onCommit={(v) => patch("overlay", { fps: v })}
          />
        </div>
      </section>
    </>
  );
}
