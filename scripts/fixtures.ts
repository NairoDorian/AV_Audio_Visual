// scripts/fixtures.ts — regenerate the cross-language fixtures from the Rust reference.
//   bun run fixtures
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const ROOT = resolve(import.meta.dirname, "..");
const r = spawnSync(
  "cargo",
  ["run", "--quiet", "-p", "av-protocol", "--example", "dump_fixtures", "--", "test/fixtures/wire"],
  { cwd: ROOT, stdio: "inherit" },
);
process.exit(r.status ?? 1);
