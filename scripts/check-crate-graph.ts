// scripts/check-crate-graph.ts — enforce the allowed internal dependency edges.
//
// The table is docs/phase-1a-blueprint.md section 3.1, as data. Reads `cargo metadata` and fails
// on any internal edge (normal, dev or build) that is not listed, and on a forbidden external
// dependency (Tauri anywhere but the composition root). Also flags any Rust source file over the
// split-review threshold so a god module is noticed early (AGENTS.md section 3.1).

import { spawnSync } from "node:child_process";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const TAG = "[crate-graph]";
const SPLIT_REVIEW_LINES = 2000;

/** crate -> internal crates it may depend on. */
export const ALLOWED: Record<string, readonly string[]> = {
  "av-protocol": [],
  "av-dsp": ["av-protocol"],
  "av-audio": ["av-protocol"],
  "av-engine": ["av-protocol", "av-dsp", "av-audio"],
  "av-render": ["av-protocol"],
  "av-platform": ["av-protocol"],
  "av-app": ["av-protocol", "av-dsp", "av-audio", "av-engine", "av-render", "av-platform"],
};

/** External crates only the composition root may use. */
const ROOT_ONLY = /^tauri($|-)/;
const COMPOSITION_ROOT = "av-app";

interface Pkg {
  name: string;
  manifest_path: string;
  dependencies: { name: string; kind: string | null }[];
}

export function violations(packages: Pkg[]): string[] {
  const out: string[] = [];
  const internal = new Set(Object.keys(ALLOWED));
  for (const pkg of packages) {
    const allowed = ALLOWED[pkg.name];
    if (!allowed) {
      out.push(`${pkg.name}: workspace member is not in the allowed-edge table`);
      continue;
    }
    for (const dep of pkg.dependencies) {
      const kind = dep.kind ?? "normal";
      if (internal.has(dep.name) && !allowed.includes(dep.name)) {
        out.push(`${pkg.name} -> ${dep.name} (${kind}) is not an allowed edge`);
      }
      if (ROOT_ONLY.test(dep.name) && pkg.name !== COMPOSITION_ROOT) {
        out.push(
          `${pkg.name} -> ${dep.name} (${kind}): only ${COMPOSITION_ROOT} may depend on Tauri`,
        );
      }
    }
  }
  return out;
}

function rustFiles(dir: string, acc: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === "target" || entry === "gen") continue;
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) rustFiles(p, acc);
    else if (p.endsWith(".rs")) acc.push(p);
  }
  return acc;
}

if (import.meta.main) {
  const meta = spawnSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
    cwd: ROOT,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (meta.status !== 0) {
    console.error(`${TAG} cargo metadata failed:\n${meta.stderr}`);
    process.exit(1);
  }
  const packages = (JSON.parse(meta.stdout) as { packages: Pkg[] }).packages;
  const bad = violations(packages);
  for (const b of bad) console.error(`${TAG} ${b}`);

  const big: string[] = [];
  for (const dir of ["crates", "src-tauri/src"]) {
    for (const f of rustFiles(resolve(ROOT, dir))) {
      const lines = readFileSync(f, "utf8").split("\n").length;
      if (lines > SPLIT_REVIEW_LINES) big.push(`${relative(ROOT, f)} (${lines} lines)`);
    }
  }
  for (const b of big)
    console.warn(`${TAG} split review: ${b} exceeds ${SPLIT_REVIEW_LINES} lines`);

  if (bad.length) process.exit(1);
  console.log(`${TAG} ${packages.length} crates, every internal edge allowed`);
}
