import { createSignal, For, Show, Match, Switch, onSettled } from "solid-js";
import Preview from "./features/visualizer/Preview";
import {
  getAppInfo,
  getAudioStatus,
  getConfig,
  inTauri,
  patchConfig,
  startSource,
  stopSource,
  type AnalysisConfig,
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
  { id: "performance", label: "Performance", blurb: "FFT preset, frame rate and idle policy." },
  { id: "diagnostics", label: "Diagnostics", blurb: "Counters, timings and build facts." },
];

const fmt = (n: number | undefined, digits = 0) => (n === undefined ? "-" : n.toFixed(digits));

/** Below this the input is too quiet to show on the default -72 dB display floor. */
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
  const [scenes, setScenes] = createSignal<Scene[]>([]);
  const [scene, setScene] = createSignal<Scene | null>(null);
  const [notice, setNotice] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [filePath, setFilePath] = createSignal("");
  const tauri = inTauri();

  const report = (e: unknown) => setError(String(e));

  onSettled(() => {
    if (!tauri) {
      setError("Not running inside Tauri (browser preview): no engine to talk to.");
      return;
    }
    getAppInfo().then(setInfo, report);
    getConfig().then(setConfig, report);
    listScenes().then(setScenes, report);
    getActiveScene().then(setScene, report);
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
      setScene(await getActiveScene());
    } catch (e) {
      report(e);
    }
  };

  const editAnalysis = async (patch: Partial<AnalysisConfig>) => {
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
                <Preview scene={scene()} onError={setError} />
              </Show>
            </section>
            <Show when={config()}>
              {(c) => (
                <section class="card form">
                  <label>
                    <span>Scene</span>
                    <select value={c().scene} onChange={(e) => chooseScene(e.currentTarget.value)}>
                      <For each={scenes()}>{(s) => <option value={s.id}>{s.name}</option>}</For>
                    </select>
                  </label>
                  <label>
                    <span>Bars</span>
                    <input
                      type="range"
                      min="8"
                      max="128"
                      value={c().analysis.barCount}
                      onChange={(e) => editAnalysis({ barCount: +e.currentTarget.value })}
                    />
                    <output>{c().analysis.barCount}</output>
                  </label>
                  <label>
                    <span>FFT preset</span>
                    <select
                      value={c().analysis.preset}
                      onChange={(e) =>
                        editAnalysis({ preset: e.currentTarget.value as AnalysisConfig["preset"] })
                      }
                    >
                      <option value="layered">Layered Super FFT (1,024 x 2^k)</option>
                      <option value="lowLatency">Low latency (1024 / 256)</option>
                      <option value="default">Default (2048 / 512)</option>
                      <option value="precision">Precision (8192 / 1024)</option>
                      <option value="lowPower">Low power (1024 / 512)</option>
                    </select>
                  </label>
                  <Show when={c().analysis.preset === "layered"}>
                    <label>
                      <span>Layers</span>
                      <input
                        type="range"
                        min="1"
                        max="6"
                        value={c().analysis.layers}
                        onChange={(e) => editAnalysis({ layers: +e.currentTarget.value })}
                      />
                      <output>{1024 << (c().analysis.layers - 1)}</output>
                    </label>
                  </Show>
                  <label>
                    <span>FFT size</span>
                    <select
                      value={String(c().analysis.fftSize)}
                      onChange={(e) => editAnalysis({ fftSize: +e.currentTarget.value })}
                    >
                      <For each={[8192, 16384, 32768, 65536, 131072]}>
                        {(n) => (
                          <option value={String(n)}>{n.toLocaleString()} (zero-padded)</option>
                        )}
                      </For>
                    </select>
                  </label>
                  <label>
                    <span>Window</span>
                    <select
                      value={c().analysis.window}
                      onChange={(e) =>
                        editAnalysis({ window: e.currentTarget.value as AnalysisConfig["window"] })
                      }
                    >
                      <option value="kaiser">Kaiser (beta {c().analysis.kaiserBeta})</option>
                      <option value="hann">Hann</option>
                      <option value="blackmanHarris">Blackman-Harris</option>
                    </select>
                  </label>
                  <label>
                    <span>Warp blend</span>
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.01"
                      value={c().analysis.warpBlend}
                      onChange={(e) => editAnalysis({ warpBlend: +e.currentTarget.value })}
                    />
                    <output>{c().analysis.warpBlend.toFixed(2)}</output>
                  </label>
                  <label class="check">
                    <input
                      type="checkbox"
                      checked={c().analysis.eq.enabled && c().analysis.eq.highShelf}
                      onChange={(e) =>
                        editAnalysis({
                          eq: {
                            ...c().analysis.eq,
                            enabled: true,
                            highShelf: e.currentTarget.checked,
                          },
                        })
                      }
                    />
                    <span>
                      High shelf +{c().analysis.eq.highGainDb} dB at {c().analysis.eq.highCutoffHz}{" "}
                      Hz
                    </span>
                  </label>
                  <label>
                    <span>Band layout</span>
                    <select
                      value={c().analysis.layout}
                      onChange={(e) =>
                        editAnalysis({ layout: e.currentTarget.value as AnalysisConfig["layout"] })
                      }
                    >
                      <option value="log">Logarithmic</option>
                      <option value="mel">Mel</option>
                      <option value="linear">Linear</option>
                    </select>
                  </label>
                  <label>
                    <span>Release (ms)</span>
                    <input
                      type="range"
                      min="20"
                      max="1000"
                      value={c().analysis.releaseMs}
                      onChange={(e) => editAnalysis({ releaseMs: +e.currentTarget.value })}
                    />
                    <output>{fmt(c().analysis.releaseMs)}</output>
                  </label>
                  <label class="check">
                    <input
                      type="checkbox"
                      checked={c().analysis.autoGain}
                      onChange={(e) => editAnalysis({ autoGain: e.currentTarget.checked })}
                    />
                    <span>Auto-gain</span>
                  </label>
                  <Show when={notice()}>
                    <p class="muted small">{notice()}</p>
                  </Show>
                </section>
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
                    <dt>Source</dt>
                    <dd>{src().label}</dd>
                    <dt>Format</dt>
                    <dd>
                      {src().sampleRate} Hz, {src().channels} ch
                    </dd>
                    <dt>State</dt>
                    <dd>
                      {status()?.capture === "failed"
                        ? "device lost: pick the source again"
                        : levelLabel(status())}
                    </dd>
                    <dt>Input peak</dt>
                    <dd>{fmt(status()?.inputPeakDbfs, 1)} dBFS</dd>
                  </dl>
                )}
              </Show>
            </section>
          </Match>

          <Match when={section() === "diagnostics"}>
            <section class="card">
              <dl class="kv">
                <dt>Name</dt>
                <dd>{info()?.name ?? "-"}</dd>
                <dt>Version</dt>
                <dd>{info()?.version ?? "-"}</dd>
                <dt>Platform</dt>
                <dd>
                  {info()?.os ?? "-"} / {info()?.arch ?? "-"} ({info()?.debug ? "debug" : "release"}
                  )
                </dd>
                <dt>FFT / hop</dt>
                <dd>
                  {fmt(status()?.fftSize)} / {fmt(status()?.hopSamples)}
                </dd>
                <dt>Bars</dt>
                <dd>{fmt(status()?.barCount)}</dd>
                <dt>Generation</dt>
                <dd>{fmt(status()?.generation)}</dd>
                <dt>Frames</dt>
                <dd>{fmt(status()?.frames)}</dd>
                <dt>DSP avg / max</dt>
                <dd>
                  {fmt(status()?.dspUsAvg, 1)} / {fmt(status()?.dspUsMax, 1)} us
                </dd>
                <dt>Overruns / dropped</dt>
                <dd>
                  {fmt(status()?.overruns)} / {fmt(status()?.droppedSamples)}
                </dd>
                <dt>Starved hops</dt>
                <dd>{fmt(status()?.starvedHops)}</dd>
                <dt>Clock</dt>
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
