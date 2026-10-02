import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const generated = resolve(root, "src/types/generated");
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
const result = spawnSync(
  cargo,
  ["test", "--manifest-path", "src-tauri/Cargo.toml", "export_bindings"],
  {
    cwd: root,
    env: { ...process.env, TS_RS_EXPORT_DIR: generated },
    stdio: "inherit",
  },
);

if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

for (const name of readdirSync(generated).filter((file) => file.endsWith(".ts"))) {
  const path = resolve(generated, name);
  const source = readFileSync(path, "utf8");
  const normalized = source.replace(/[ \t]+(?=\r?$)/gm, "");
  if (normalized !== source) writeFileSync(path, normalized);
}
