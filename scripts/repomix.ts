// scripts/repomix.ts — pack the repository into repomix-output.xml (git-ignored) for review and
// for AI context. `--check` fails when the pack is older than the newest tracked file.
//
// Provenance: adapted from ZER0 `scripts/repomix.ts` (MIT); repomix is a pinned devDependency
// here instead of a `bunx` download.

import { spawnSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const OUTPUT = resolve(ROOT, "repomix-output.xml");
const TAG = "[repomix]";

function newestTrackedMtime(): number {
  const tracked = spawnSync("git", ["ls-files", "-z"], { cwd: ROOT, encoding: "utf8" });
  if (tracked.status !== 0) throw new Error(`git ls-files failed: ${tracked.stderr}`);
  let newest = 0;
  for (const f of tracked.stdout.split("\0")) {
    if (!f) continue;
    try {
      newest = Math.max(newest, statSync(resolve(ROOT, f)).mtimeMs);
    } catch {
      // A tracked file missing on disk cannot be newer than the pack.
    }
  }
  return newest;
}

if (process.argv.includes("--check")) {
  if (!existsSync(OUTPUT) || newestTrackedMtime() > statSync(OUTPUT).mtimeMs) {
    console.error(`${TAG} pack missing or stale; run \`bun run repomix\``);
    process.exit(1);
  }
  console.log(`${TAG} up to date`);
} else {
  const r = spawnSync("repomix", ["--config", "repomix.config.json", "--quiet"], {
    cwd: ROOT,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (r.status !== 0) process.exit(r.status ?? 1);
  console.log(`${TAG} wrote repomix-output.xml`);
}
