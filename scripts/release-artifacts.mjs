import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { basename, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const PLATFORMS = ["darwin-universal", "windows-x86_64", "linux-x86_64"];

function filesUnder(root) {
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = join(root, entry.name);
    return entry.isDirectory() ? filesUnder(path) : entry.isFile() ? [path] : [];
  });
}

export function collectArtifacts(root, destination, platform) {
  if (!PLATFORMS.includes(platform)) throw new Error(`Unsupported platform: ${platform}`);
  mkdirSync(destination, { recursive: true });
  const artifacts = filesUnder(root).filter((path) =>
    /\.(dmg|app\.tar\.gz|exe|msi|AppImage|deb|rpm)(\.sig)?$/.test(path),
  );
  if (!artifacts.length) throw new Error("No bundle artifacts found");
  for (const path of artifacts) {
    const name = `${platform}-${basename(path).replace(/[^a-zA-Z0-9._+-]/g, "_")}`;
    const target = join(destination, name);
    if (existsSync(target)) throw new Error(`Duplicate artifact: ${name}`);
    copyFileSync(path, target);
  }
}

export function createManifest(root, version, baseUrl, notes = "") {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version))
    throw new Error("Invalid release version");
  const url = new URL(baseUrl);
  if (url.protocol !== "https:") throw new Error("Release URLs must use HTTPS");
  const names = readdirSync(root).sort();
  const platforms = {};
  for (const platform of PLATFORMS) {
    const extension = platform.startsWith("darwin")
      ? ".app.tar.gz"
      : platform.startsWith("windows")
        ? "-setup.exe"
        : ".AppImage";
    const candidates = names.filter(
      (name) => name.startsWith(`${platform}-`) && name.endsWith(extension),
    );
    if (candidates.length !== 1) throw new Error(`Expected one updater artifact for ${platform}`);
    const asset = candidates[0];
    const signature = readFileSync(join(root, `${asset}.sig`), "utf8").trim();
    if (!signature) throw new Error(`Empty updater signature: ${asset}`);
    platforms[platform] = {
      signature,
      url: `${baseUrl.replace(/\/$/, "")}/${encodeURIComponent(asset)}`,
    };
  }
  platforms["darwin-aarch64"] = platforms["darwin-universal"];
  platforms["darwin-x86_64"] = platforms["darwin-universal"];
  return { version, notes, pub_date: new Date().toISOString(), platforms };
}

export function writeReleaseMetadata(root, version, baseUrl, notes) {
  const manifest = createManifest(root, version, baseUrl, notes);
  writeFileSync(join(root, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  const sums = readdirSync(root)
    .sort()
    .filter((name) => name !== "SHA256SUMS" && !name.endsWith(".asc"))
    .map(
      (name) =>
        `${createHash("sha256")
          .update(readFileSync(join(root, name)))
          .digest("hex")}  ${name}`,
    )
    .join("\n");
  writeFileSync(join(root, "SHA256SUMS"), `${sums}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [command, root, target, value, notesPath] = process.argv.slice(2);
  if (command === "collect") collectArtifacts(root, target, value);
  else if (command === "manifest")
    writeReleaseMetadata(
      root,
      target.replace(/^v/, ""),
      value,
      notesPath ? readFileSync(notesPath, "utf8") : "",
    );
  else
    throw new Error(
      "Usage: release-artifacts.mjs collect <bundle-dir> <output-dir> <platform> | manifest <artifact-dir> <version> <base-url> [notes-file]",
    );
}
