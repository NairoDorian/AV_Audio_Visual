import { createSignal, For, Show, Match, Switch, onSettled } from "solid-js";
import Preview from "./features/visualizer/Preview";
import AnalysisPanel from "./features/analysis/AnalysisPanel";
import LookPanel from "./features/visualizer/LookPanel";
import { LOOK_HELP } from "./features/visualizer/help";
import InfoTip from "./components/InfoTip";
import { DIAG_HELP } from "./features/diagnostics/help";

/** A readout label with its info bubble. */
function Dt(props: { label: string }) {
  const help = () => DIAG_HELP[props.label];
  return (
    <dt>
      <span class="param-label">
        <span>{props.label}</span>
        <Show when={help()}>{(h) => <InfoTip title={props.label} help={h()} />}</Show>
      </span>
    </dt>
  );
}
import {
  getAppInfo,
  getAudioStatus,
  getConfig,
  getDefaults,
  inTauri,
  patchConfig,
  startSource,
  stopSource,
  getActiveScene,
  listScenes,
  type AppConfig,
  type AppInfo,
  type AudioStatus,
  type Scene,
} from "./lib/ipc";
import { NAME } from "./lib/identity";

type SectionId = "visualizer" | "audio" | "displays" | "performance" | "diagnostics";

const SECTIONS: ReadonlyArray<{ id: SectionId; label: string; blurb: string }> = [
  { id: "visualizer", label: "Visualizer", blurb: "Live preview and analysis settings." },
  { id: "audio", label: "Audio", blurb: "Source and capture status." },
  { id: "displays", label: "Displays", blurb: "Background, taskbar and overlay placements." },
  { id: "performance", label: "Performance", blurb: "Frame rate and idle policy." },
  { id: "diagnostics", label: "Diagnostics", blurb: "Counters, timings and build facts." },
];

const fmt = (n: number | undefined, digits = 0) => (n === undefined ? "-" : n.toFixed(digits));

/** Below this the input is too quiet to read well on the default 80 dB range. */
const QUIET_DBFS = -60;

function levelLabel(s: AudioStatus | null): string {
  if (!s?.source) return "idle";
  if (s.capture === "failed") return "device lost";
  if (s.silent) return "silent";
  return s.inputPeakDbfs < QUIET_DBFS ? "quiet" : "audible";
}

/** Status is low-rate data: polled at 1 Hz, never per frame. */
const STATUS_POLL_MS = 1000;

export default function App() {
  // Sections are local UI state; no router in v0.1 (blueprint section 7).
  const [section, setSection] = createSignal<SectionId>("visualizer");
  const [info, setInfo] = createSignal<AppInfo | null>(null);
  const [status, setStatus] = createSignal<AudioStatus | null>(null);
  const [config, setConfig] = createSignal<AppConfig | null>(null);
  const [defaults, setDefaults] = createSignal<AppConfig | null>(null);
  const [scenes, setScenes] = createSignal<Scene[]>([]);
  const [scene, setScene] = createSignal<Scene | null>(null);
  const [notice, setNotice] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [filePath, setFilePath] = createSignal("");
  // Preview-only display count (ephemeral UI state, never persisted).
  const [displayCount, setDisplayCount] = createSignal(64);
  const tauri = inTauri();

  const report = (e: unknown) => setError(String(e));

  onSettled(() => {
    if (!tauri) {
      setError("Not running inside Tauri (browser preview): no engine to talk to.");
      return;
    }
    getAppInfo().then(setInfo, report);
    getConfig().then(setConfig, report);
    getDefaults().then(setDefaults, report);
    listScenes().then(setScenes, report);
    getActiveScene().then((s) => {
      setScene(s);
      setDisplayCount(s.style === "concentric" ? s.ring.bins : s.barCount);
    }, report);
    const poll = () => getAudioStatus().then(setStatus, report);
    void poll();
    const timer = setInterval(poll, STATUS_POLL_MS);
    return () => clearInterval(timer);
  });

  const play = async (request: Parameters<typeof startSource>[0]) => {
    setError(null);
    try {
      await startSource(request);
      setStatus(await getAudioStatus());
    } catch (e) {
      report(e);
    }
  };

  const stop = async () => {
    await stopSource().catch(report);
    setStatus(await getAudioStatus().catch(() => null));
  };

  /** Optimistic edit; Rust answers with the applied (sanitized) value. */
  const chooseScene = async (id: string) => {
    try {
      const applied = await patchConfig({ scene: id });
      setConfig(applied.config);
      const s = await getActiveScene();
      setScene(s);
      setDisplayCount(s.style === "concentric" ? s.ring.bins : s.barCount);
    } catch (e) {
      report(e);
    }
  };

  const editAnalysis = async (patch: Record<string, unknown>) => {
    try {
      const applied = await patchConfig({ analysis: patch });
      setConfig(applied.config);
      const changed = [...applied.report.clamped, ...applied.report.normalized];
      setNotice(changed.length ? `Adjusted by the engine: ${changed.join(", ")}` : null);
    } catch (e) {
      report(e);
    }
  };

  const current = () => SECTIONS.find((s) => s.id === section()) ?? SECTIONS[0]!;

  return (
    <div class="shell">
      <aside class="sidebar">
        <div class="brand">{NAME}</div>
        <nav>
          <For each={SECTIONS}>
            {(s) => (
              <button
                type="button"
                class={["nav-item", { active: section() === s.id }]}
                onClick={() => setSection(s.id)}
              >
                {s.label}
              </button>
            )}
          </For>
        </nav>
      </aside>

      <main class="content">
        <header>
          <h1>{current().label}</h1>
          <p class="muted">{current().blurb}</p>
        </header>

        <Show when={error()}>
          <div class="card error" role="alert">
            {error()}
          </div>
        </Show>

        <Switch fallback={<section class="card muted">Not implemented yet.</section>}>
          <Match when={section() === "visualizer"}>
            <section class="card preview-card">
              <Show
                when={status()?.source}
                fallback={
                  <p class="muted">
                    No source is playing. Pick <b>System audio</b> in <b>Audio</b>.
                  </p>
                }
              >
                <Preview scene={scene()} displayCount={displayCount()} onError={setError} />
              </Show>
            </section>
            <Show when={config()}>
              {(c) => (
                <>
                  <section class="card form">
                    <label>
                      <span class="param-label">
                        <span>Scene</span>
                        <InfoTip title="Scene" help={LOOK_HELP["scene"]!} />
                      </span>
                      <select
                        value={c().scene}
                        onChange={(e) => chooseScene(e.currentTarget.value)}
                      >
                        <For each={scenes()}>{(s) => <option value={s.id}>{s.name}</option>}</For>
                      </select>
                      <span />
                    </label>
                    <label>
                      <span class="param-label">
                        <span>
                          {scene()?.style === "concentric" ? "Ring bins" : "Display bars"}
                        </span>
                        <InfoTip
                          title={scene()?.style === "concentric" ? "Ring bins" : "Display bars"}
                          help={LOOK_HELP["displayCount"]!}
                          facts={["Range: 4 to 4,096"]}
                        />
                      </span>
                      <input
                        type="range"
                        min="0"
                        max="1000"
                        value={Math.round(
                          (Math.log(displayCount() / 4) / Math.log(4096 / 4)) * 1000,
                        )}
                        onInput={(e) =>
                          setDisplayCount(
                            Math.round(4 * Math.pow(4096 / 4, +e.currentTarget.value / 1000)),
                          )
                        }
                      />
                      <output>{displayCount()}</output>
                    </label>
                    <p class="muted small">
                      Each display bar shows the loudest of the output bins it covers (preview only;
                      scene editing arrives with scene files).
                    </p>
                  </section>
                  <Show when={scene()}>
                    {(s) => (
                      <LookPanel
                        scene={s()}
                        onUpdate={(update) => setScene((prev) => (prev ? update(prev) : prev))}
                      />
                    )}
                  </Show>
                  <AnalysisPanel
                    analysis={c().analysis}
                    defaults={defaults()?.analysis ?? null}
                    sampleRate={status()?.source?.sampleRate ?? 48_000}
                    onEdit={editAnalysis}
                    onApplied={(applied) => {
                      setConfig(applied.config);
                      setNotice(null);
                    }}
                    onError={report}
                  />
                  <Show when={notice()}>
                    <p class="muted small">{notice()}</p>
                  </Show>
                </>
              )}
            </Show>
          </Match>

          <Match when={section() === "audio"}>
            <section class="card form">
              <div class="row">
                <button type="button" class="primary" onClick={() => play({ kind: "system" })}>
                  System audio
                </button>
                <button type="button" onClick={() => play({ kind: "microphone" })}>
                  Microphone
                </button>
                <button type="button" onClick={() => play({ kind: "demo" })}>
                  Demo track
                </button>
                <button type="button" onClick={stop} disabled={!status()?.source}>
                  Stop
                </button>
              </div>
              <div class="row">
                <input
                  class="grow"
                  type="text"
                  placeholder="Path to an audio file (WAV, FLAC, MP3, M4A, OGG)"
                  value={filePath()}
                  onInput={(e) => setFilePath(e.currentTarget.value)}
                />
                <button
                  type="button"
                  disabled={!filePath().trim()}
                  onClick={() => play({ kind: "file", path: filePath().trim() })}
                >
                  Play file
                </button>
              </div>
              <p class="muted small">
                System audio captures what this computer plays (WASAPI loopback on Windows). The
                microphone is used only when you pick it.
              </p>
            </section>
            <section class="card">
              <Show when={status()?.source} fallback={<p class="muted">Nothing playing.</p>}>
                {(src) => (
                  <dl class="kv">
                    <Dt label="Source" />
                    <dd>{src().label}</dd>
                    <Dt label="Format" />
                    <dd>
                      {src().sampleRate} Hz, {src().channels} ch
                    </dd>
                    <Dt label="State" />
                    <dd>
                      {status()?.capture === "failed"
                        ? "device lost: pick the source again"
                        : levelLabel(status())}
                    </dd>
                    <Dt label="Input peak" />
                    <dd>{fmt(status()?.inputPeakDbfs, 1)} dBFS</dd>
                  </dl>
                )}
              </Show>
            </section>
          </Match>

          <Match when={section() === "diagnostics"}>
            <section class="card">
              <dl class="kv">
                <Dt label="Name" />
                <dd>{info()?.name ?? "-"}</dd>
                <Dt label="Version" />
                <dd>{info()?.version ?? "-"}</dd>
                <Dt label="Platform" />
                <dd>
                  {info()?.os ?? "-"} / {info()?.arch ?? "-"} ({info()?.debug ? "debug" : "release"}
                  )
                </dd>
                <Dt label="FFT / hop" />
                <dd>
                  {fmt(status()?.fftSize)} / {fmt(status()?.hopSamples)}
                </dd>
                <Dt label="Layers / first window" />
                <dd>
                  {fmt(status()?.layers)} / {fmt(status()?.windowSamples)}
                  {status()?.threads && status()!.threads > 1
                    ? ` (${status()!.threads} threads)`
                    : ""}
                </dd>
                <Dt label="Output bins" />
                <dd>{fmt(status()?.outputBins)}</dd>
                <Dt label="Peak" />
                <dd>{fmt(status()?.peakHz, 1)} Hz</dd>
                <Show when={status()?.features}>
                  {(f) => (
                    <>
                      <Dt label="Centroid / rolloff" />
                      <dd>
                        {fmt(f().centroidHz)} / {fmt(f().rolloffHz)} Hz
                      </dd>
                      <Dt label="Flatness / flux" />
                      <dd>
                        {fmt(f().flatness, 3)} / {fmt(f().flux, 3)}
                      </dd>
                      <Dt label="RMS / bass / mid / high" />
                      <dd>
                        {fmt(f().rmsDb, 1)} / {fmt(f().bassDb, 1)} / {fmt(f().midDb, 1)} /{" "}
                        {fmt(f().highDb, 1)} dB
                      </dd>
                    </>
                  )}
                </Show>
                <Dt label="Generation" />
                <dd>{fmt(status()?.generation)}</dd>
                <Dt label="Frames" />
                <dd>{fmt(status()?.frames)}</dd>
                <Dt label="DSP avg / max" />
                <dd>
                  {fmt(status()?.dspUsAvg, 1)} / {fmt(status()?.dspUsMax, 1)} us
                </dd>
                <Dt label="Overruns / dropped" />
                <dd>
                  {fmt(status()?.overruns)} / {fmt(status()?.droppedSamples)}
                </dd>
                <Dt label="Starved hops" />
                <dd>{fmt(status()?.starvedHops)}</dd>
                <Dt label="Clock" />
                <dd>{status()?.clock ?? "-"}</dd>
              </dl>
            </section>
          </Match>
        </Switch>
      </main>

      <footer class="statusbar">
        <span>Source: {status()?.source?.label ?? "none"}</span>
        <span>DSP: {fmt(status()?.dspUsAvg, 1)} us</span>
        <span>Frames: {fmt(status()?.frames)}</span>
        <span>
          {levelLabel(status())}
          {status()?.source ? `, in ${fmt(status()?.inputPeakDbfs, 0)} dBFS` : ""}
        </span>
      </footer>
    </div>
  );
}
