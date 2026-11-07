// scripts/pre-commit.ts — the gate and the routine around it.
//
//   bun run precommit          THE GATE: fast checks only. The git hook runs this; it must stay
//                              fast enough that nobody reaches for `--no-verify`.
//   bun run precommit:full     The gate plus clippy -D warnings and the Rust test suite.
//   bun run precommit:routine  Every dependency to its newest version first (`bun run update`),
//                              then `precommit:full` on the result.
//
// Provenance: ported from ZER0 `scripts/pre-commit.ts` (MIT) and adapted: the crate-graph
// check is new, the step list follows docs/phase-1a-blueprint.md section 8.
//
// The checks read the working tree, not the index: stage what you mean to commit and keep the
// rest clean or stashed. The only files the gate itself stages are the identity mirrors that
// `meta:sync` rewrites, staged with `git add -u -- <paths>` so a selective staging is never
// widened. A translations step joins when the Studio gains locale bundles.

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { identityPaths } from "./app-meta";

const ROOT = resolve(import.meta.dirname, "..");
const TAG = "[precommit]";
const args = process.argv.slice(2);
const routine = args.includes("--routine");
const full = routine || args.includes("--full");
/** CI has no use for a repomix pack. */
const skipRepomix = args.includes("--no-repomix") || process.env.CI === "true";

interface Step {
  label: string;
  /** argv for `bun`. */
  argv: string[];
  heavy?: boolean;
  last?: boolean;
  writes?: boolean;
}

const ROUTINE_DEPS: Step = {
  label: "deps -> newest, validated",
  argv: ["run", "update"],
  writes: true,
};

const STEPS: Step[] = [
  { label: "identity mirrors", argv: ["run", "meta:sync"], writes: true },
  { label: "identity in sync", argv: ["run", "meta:check"] },
  { label: "crate graph", argv: ["run", "check:crates"] },
  { label: "lint", argv: ["run", "lint"] },
  { label: "typecheck", argv: ["run", "typecheck"] },
  { label: "unit + parity tests", argv: ["run", "test"] },
  { label: "format", argv: ["run", "format:check"] },
  { label: "rust lints", argv: ["run", "lint:rust"], heavy: true },
  { label: "rust tests", argv: ["run", "test:rust"], heavy: true },
  { label: "repomix pack", argv: ["run", "repomix"], last: true },
];

const ROUTINE_PATHS = [
  "package.json",
  "bun.lock",
  "Cargo.toml",
  "Cargo.lock",
  "crates",
  "src-tauri/Cargo.toml",
];

function run(step: Step): boolean {
  const started = Date.now();
  console.log(`${TAG} ${step.label} ...`);
  const result = spawnSync("bun", step.argv, {
    cwd: ROOT,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  const ms = Date.now() - started;
  if (result.status !== 0) {
    console.error(`${TAG} ${step.label} FAILED after ${ms} ms`);
    return false;
  }
  console.log(`${TAG} ${step.label} ok (${ms} ms)`);
  return true;
}

function stage(paths: readonly string[]): void {
  const staged = spawnSync("git", ["add", "-u", "--", ...paths], { cwd: ROOT });
  if (staged.status !== 0)
    console.error(`${TAG} could not stage regenerated files; stage them by hand`);
}

function warnIfHookMissing(): void {
  const hooks = spawnSync("git", ["config", "--get", "core.hooksPath"], {
    cwd: ROOT,
    encoding: "utf8",
  });
  if (hooks.status === 0 && hooks.stdout.trim() === ".githooks") return;
  console.log(
    `${TAG} note: the git hook is not installed in this clone; run \`bun run hooks:install\` once.`,
  );
}

const selected = STEPS.filter((s) => (s.heavy ? full : true)).filter(
  (s) => !(s.last && skipRepomix),
);
if (!process.env.CI) warnIfHookMissing();

const startedAll = Date.now();
if (routine) {
  if (!run(ROUTINE_DEPS)) process.exit(1);
  stage(ROUTINE_PATHS);
}
for (const step of selected.filter((s) => !s.last)) {
  if (!run(step)) process.exit(1);
}
if (!process.env.CI && selected.some((s) => s.writes)) stage(identityPaths());
for (const step of selected.filter((s) => s.last)) {
  if (!run(step)) process.exit(1);
}

const seconds = ((Date.now() - startedAll) / 1000).toFixed(1);
console.log(`${TAG} all ${selected.length + (routine ? 1 : 0)} steps passed in ${seconds}s`);
