// Settings parity (Track A step A5): the defaults and limit tables are generated from the Rust
// reference (`bun run fixtures`). Every limit must name a real numeric field of the defaults,
// and every default must sit inside its range, so the Studio and Rust agree on what is valid.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

interface LimitRow {
  path: string;
  min: number;
  max: number;
  integer: boolean;
  default: number;
}

const DIR = join(import.meta.dirname, "fixtures", "settings");
const read = (name: string) => JSON.parse(readFileSync(join(DIR, name), "utf8"));

function resolvePath(doc: unknown, path: string): unknown {
  return path.split(".").reduce<unknown>((node, key) => {
    if (node === null || typeof node !== "object") return undefined;
    return (node as Record<string, unknown>)[key];
  }, doc);
}

const VERSIONS = { config: 2, scene: 1 } as const;

for (const kind of ["config", "scene"] as const) {
  describe(`${kind} settings parity`, () => {
    const defaults = read(`${kind}.defaults.json`);
    const limits = read(`${kind}.limits.json`) as LimitRow[];
    const versionKey = kind === "config" ? "schemaVersion" : "formatVersion";

    test("the defaults carry the current version", () => {
      expect(defaults[versionKey]).toBe(VERSIONS[kind]);
    });

    test("every limit names a numeric field whose default matches", () => {
      for (const row of limits) {
        const value = resolvePath(defaults, row.path);
        expect(typeof value, row.path).toBe("number");
        expect(Math.fround(value as number), row.path).toBe(Math.fround(row.default));
      }
    });

    test("every default is inside its range and integers are whole", () => {
      for (const row of limits) {
        expect(row.min, row.path).toBeLessThanOrEqual(row.max);
        expect(row.default, row.path).toBeGreaterThanOrEqual(row.min);
        expect(row.default, row.path).toBeLessThanOrEqual(row.max);
        if (row.integer) expect(Number.isInteger(row.default), row.path).toBe(true);
      }
    });

    test("limit paths are unique", () => {
      const paths = limits.map((r) => r.path);
      expect(new Set(paths).size).toBe(paths.length);
    });
  });
}

test("the built-in preset is exactly the default analysis", () => {
  const preset = read("preset.defaults.json");
  expect(preset.presetVersion).toBe(1);
  expect(preset.id).toBe("default");
  expect(preset.analysis).toEqual(read("config.defaults.json").analysis);
});

test("scenes exclude machine-specific settings", () => {
  const scene = JSON.stringify(read("scene.defaults.json")).toLowerCase();
  for (const word of ["device", "monitor", "hotkey", "gpu", "anchor"]) {
    expect(scene.includes(word), word).toBe(false);
  }
});
