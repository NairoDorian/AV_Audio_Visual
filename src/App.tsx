import { createSignal, For, Show, Match, Switch, onSettled } from "solid-js";
import { getAppInfo, inTauri, type AppInfo } from "./lib/ipc";

type SectionId = "visualizer" | "audio" | "displays" | "performance" | "diagnostics";

const SECTIONS: ReadonlyArray<{ id: SectionId; label: string; blurb: string }> = [
  {
    id: "visualizer",
    label: "Visualizer",
    blurb: "Scenes, styles, palettes and the live preview.",
  },
  { id: "audio", label: "Audio", blurb: "Source, device and capture status." },
  { id: "displays", label: "Displays", blurb: "Background, taskbar and overlay placements." },
  { id: "performance", label: "Performance", blurb: "FFT preset, frame rate and idle policy." },
  { id: "diagnostics", label: "Diagnostics", blurb: "Counters, timings and the session log." },
];

export default function App() {
  // Sections are local UI state; no router in v0.1 (blueprint section 7).
  const [section, setSection] = createSignal<SectionId>("visualizer");
  const [info, setInfo] = createSignal<AppInfo | null>(null);
  const [infoError, setInfoError] = createSignal<string | null>(null);

  onSettled(() => {
    if (!inTauri()) {
      setInfoError("Not running inside Tauri (browser preview).");
      return;
    }
    getAppInfo().then(setInfo, (err: unknown) => setInfoError(String(err)));
  });

  const current = () => SECTIONS.find((s) => s.id === section()) ?? SECTIONS[0]!;

  return (
    <div class="shell">
      <aside class="sidebar">
        <div class="brand">AV_AudioVisual</div>
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
        <section class="card">
          <Switch fallback={<p class="muted">Not implemented yet.</p>}>
            <Match when={section() === "diagnostics"}>
              <Show
                when={info()}
                fallback={<p class="muted">{infoError() ?? "Loading app info..."}</p>}
              >
                {(i) => (
                  <dl class="kv">
                    <dt>Name</dt>
                    <dd>{i().name}</dd>
                    <dt>Version</dt>
                    <dd>{i().version}</dd>
                    <dt>Identifier</dt>
                    <dd>{i().identifier}</dd>
                    <dt>Platform</dt>
                    <dd>
                      {i().os} / {i().arch}
                    </dd>
                    <dt>Build</dt>
                    <dd>{i().debug ? "debug" : "release"}</dd>
                  </dl>
                )}
              </Show>
            </Match>
          </Switch>
        </section>
      </main>

      <footer class="statusbar">
        <span>Source: none</span>
        <span>FPS: -</span>
        <span>DSP: - us</span>
        <span>Idle: -</span>
      </footer>
    </div>
  );
}
