import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { plan, readMeta } from "../scripts/app-meta";
import { violations } from "../scripts/check-crate-graph";

const pkg = (name: string, deps: string[], kind: string | null = null) => ({
  name,
  manifest_path: "",
  dependencies: deps.map((d) => ({ name: d, kind })),
});

describe("crate graph", () => {
  test("the blueprint edges pass", () => {
    expect(
      violations([pkg("av-protocol", ["serde"]), pkg("av-engine", ["av-dsp", "av-audio"])]),
    ).toEqual([]);
  });

  test("a forbidden internal edge fails, dev-dependencies included", () => {
    expect(violations([pkg("av-render", ["av-platform"])])).toHaveLength(1);
    expect(violations([pkg("av-dsp", ["av-audio"], "dev")])).toHaveLength(1);
  });

  test("only the composition root may use Tauri", () => {
    expect(violations([pkg("av-platform", ["tauri"])])).toHaveLength(1);
    expect(violations([pkg("av-app", ["tauri", "tauri-build"])])).toEqual([]);
  });

  test("an unknown workspace member fails", () => {
    expect(violations([pkg("av-surprise", [])])).toHaveLength(1);
  });
});

describe("identity", () => {
  test("the committed mirrors are in sync", () => {
    expect(
      plan()
        .filter((p) => p.drift)
        .map((p) => p.path),
    ).toEqual([]);
  });

  test("invalid metadata is rejected", () => {
    const dir = mkdtempSync(join(tmpdir(), "av-meta-"));
    const base = {
      name: "X",
      slug: "x",
      identifier: "com.example.x",
      version: "1.0.0",
      description: "d",
      publisher: "p",
      repository: "r",
      license: "MIT",
    };
    writeFileSync(join(dir, "app.meta.json"), JSON.stringify({ ...base, version: "one" }));
    expect(() => readMeta(dir)).toThrow(/semver/);
    writeFileSync(join(dir, "app.meta.json"), JSON.stringify({ ...base, identifier: "x" }));
    expect(() => readMeta(dir)).toThrow(/reverse-DNS/);
    writeFileSync(join(dir, "app.meta.json"), JSON.stringify(base));
    expect(readMeta(dir).name).toBe("X");
  });
});
