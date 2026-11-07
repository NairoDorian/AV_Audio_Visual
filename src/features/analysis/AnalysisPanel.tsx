// The analysis settings: every FFT CHOP parameter on its page, always visible (inactive ones are
// greyed, never hidden), plus presets. A preset is a starting point only: applying it copies its
// settings and every parameter stays editable. Edits are optimistic; Rust answers with the
// applied (sanitized) value, which the parent mirrors.
import { createSignal, For, Match, Show, Switch, onSettled } from "solid-js";
import {
  applyPreset,
  deletePreset,
  listPresets,
  savePreset,
  type AnalysisConfig,
  type Applied,
  type PresetInfo,
} from "../../lib/ipc";
import {
  PAGES,
  SLIDER_STEPS,
  fromSlider,
  patchFor,
  readParam,
  roundTo,
  summary,
  toSlider,
  type NumberParam,
  type PageId,
  type Param,
} from "./params";

export default function AnalysisPanel(props: {
  analysis: AnalysisConfig;
  sampleRate: number;
  onEdit: (patch: Record<string, unknown>) => void;
  onApplied: (applied: Applied) => void;
  onError: (message: string) => void;
}) {
  const [page, setPage] = createSignal<PageId>("spectrum");
  const [presets, setPresets] = createSignal<PresetInfo[]>([]);
  const [selected, setSelected] = createSignal("default");
  const [name, setName] = createSignal("");

  const refresh = () => listPresets().then(setPresets, (e) => props.onError(String(e)));
  onSettled(() => {
    void refresh();
  });

  const apply = async () => {
    try {
      props.onApplied(await applyPreset(selected()));
    } catch (e) {
      props.onError(String(e));
    }
  };
  const save = async () => {
    try {
      const info = await savePreset(name().trim());
      await refresh();
      setSelected(info.id);
      setName("");
    } catch (e) {
      props.onError(String(e));
    }
  };
  const remove = async () => {
    try {
      await deletePreset(selected());
      setSelected("default");
      await refresh();
    } catch (e) {
      props.onError(String(e));
    }
  };
  const isBuiltin = () => presets().find((p) => p.id === selected())?.builtin ?? true;

  const edit = (key: string, value: unknown) => props.onEdit(patchFor(key, value));
  const active = (p: Param) => (p.when ? p.when(props.analysis) : true);
  const params = () => PAGES.find((p) => p.id === page())?.params ?? [];

  return (
    <section class="card analysis">
      <div class="row presets">
        <select
          aria-label="Preset"
          value={selected()}
          onChange={(e) => setSelected(e.currentTarget.value)}
        >
          <For each={presets()} keyed={(p) => p.id}>
            {(p) => (
              <option value={p().id}>{p().builtin ? `${p().name} (built-in)` : p().name}</option>
            )}
          </For>
        </select>
        <button type="button" onClick={apply}>
          Apply preset
        </button>
        <button type="button" onClick={remove} disabled={isBuiltin()}>
          Delete
        </button>
        <input
          class="grow"
          type="text"
          placeholder="Save current settings as..."
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && name().trim() && void save()}
        />
        <button type="button" class="primary" disabled={!name().trim()} onClick={save}>
          Save preset
        </button>
      </div>

      <p class="muted small summary">{summary(props.analysis, props.sampleRate)}</p>

      <div class="tabs" role="tablist">
        <For each={PAGES}>
          {(p) => (
            <button
              type="button"
              role="tab"
              aria-selected={page() === p.id ? "true" : "false"}
              class={["tab", { active: page() === p.id }]}
              onClick={() => setPage(p.id)}
            >
              {p.label}
            </button>
          )}
        </For>
      </div>

      <div class="params">
        <For each={params()}>
          {(p) => (
            <div class={["param", { inactive: !active(p) }]} title={p.help ?? ""}>
              <span class="param-label">{p.label}</span>
              <Switch>
                <Match when={p.kind === "number"}>
                  <NumberRow
                    p={p as NumberParam}
                    value={readParam(props.analysis, p.key) as number}
                    onCommit={(v) => edit(p.key, v)}
                  />
                </Match>
                <Match when={p.kind === "select" ? p : undefined}>
                  {(s) => (
                    <select
                      class="param-wide"
                      value={String(readParam(props.analysis, p.key))}
                      onChange={(e) =>
                        edit(
                          p.key,
                          s().numeric ? Number(e.currentTarget.value) : e.currentTarget.value,
                        )
                      }
                    >
                      <For each={s().options}>
                        {(o) => <option value={String(o.value)}>{o.label}</option>}
                      </For>
                    </select>
                  )}
                </Match>
                <Match when={p.kind === "toggle"}>
                  <input
                    type="checkbox"
                    checked={Boolean(readParam(props.analysis, p.key))}
                    onChange={(e) => edit(p.key, e.currentTarget.checked)}
                  />
                </Match>
              </Switch>
            </div>
          )}
        </For>
      </div>
      <Show when={page() === "spectrum"}>
        <p class="muted small">
          Every layer uses the same FFT size; layer k reads 2^(k-1) times the first window and is
          scaled by its window sum, so all layers average at one level.
        </p>
      </Show>
    </section>
  );
}

/** Slider over the comfortable span plus a number field that reaches the hard range. */
function NumberRow(props: { p: NumberParam; value: number; onCommit: (v: number) => void }) {
  return (
    <>
      <input
        type="range"
        min="0"
        max={String(SLIDER_STEPS)}
        value={toSlider(props.p, props.value)}
        onChange={(e) => props.onCommit(fromSlider(props.p, +e.currentTarget.value))}
      />
      <span class="param-value">
        <input
          type="number"
          min={props.p.min}
          max={props.p.max}
          step={props.p.step}
          value={props.value}
          onChange={(e) => {
            const v = Number(e.currentTarget.value);
            if (Number.isFinite(v)) props.onCommit(roundTo(props.p, v));
          }}
        />
        <Show when={props.p.unit}>
          <span class="unit">{props.p.unit}</span>
        </Show>
      </span>
    </>
  );
}
