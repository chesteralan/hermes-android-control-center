import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
const metadata = spawnSync(
  cargo,
  ["metadata", "--no-deps", "--format-version", "1", "--manifest-path", "src-tauri/Cargo.toml"],
  { encoding: "utf8" },
);

if (metadata.error) throw metadata.error;
if (metadata.status !== 0) process.exit(metadata.status ?? 1);

const packageVersion = JSON.parse(readFileSync("package.json", "utf8")).version;
const tauriVersion = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
const cargoPackage = JSON.parse(metadata.stdout).packages.find((entry) =>
  entry.manifest_path.replaceAll("\\", "/").endsWith("/src-tauri/Cargo.toml"),
);

if (!cargoPackage) throw new Error("Cargo metadata did not include src-tauri/Cargo.toml");

const versions = {
  "package.json": packageVersion,
  "src-tauri/Cargo.toml": cargoPackage.version,
  "src-tauri/tauri.conf.json": tauriVersion,
};

if (new Set(Object.values(versions)).size !== 1) {
  console.error("Application versions must match:");
  for (const [file, version] of Object.entries(versions)) {
    console.error(`  ${file}: ${version}`);
  }
  process.exit(1);
}

const tagVersion = process.argv[2]?.replace(/^v/, "");
if (tagVersion && tagVersion !== packageVersion) {
  console.error(
    `Release tag ${process.argv[2]} does not match application version ${packageVersion}.`,
  );
  process.exit(1);
}

console.log(`Application version ${packageVersion} is synchronized.`);
