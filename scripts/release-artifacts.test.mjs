import { strict as assert } from "node:assert";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { collectArtifacts, createManifest, writeReleaseMetadata } from "./release-artifacts.mjs";

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "hacc-release-"));
  for (const name of [
    "darwin-universal-Hermes.app.tar.gz",
    "windows-x86_64-Hermes_1.1.0_x64-setup.exe",
    "linux-x86_64-Hermes_1.1.0_amd64.AppImage",
  ]) {
    writeFileSync(join(root, name), "installer");
    writeFileSync(join(root, `${name}.sig`), "signed-update");
  }
  return root;
}

test("one manifest includes all platforms and universal Mac aliases", () => {
  const root = fixture();
  try {
    const result = createManifest(
      root,
      "1.1.0",
      "https://github.com/example/app/releases/download/v1.1.0",
    );
    assert.equal(Object.keys(result.platforms).length, 5);
    assert.deepEqual(result.platforms["darwin-aarch64"], result.platforms["darwin-x86_64"]);
    assert.match(result.platforms["windows-x86_64"].url, /-setup\.exe$/);
    writeReleaseMetadata(root, "1.1.0", "https://example.com/v1.1.0", "Notes");
    assert.match(readFileSync(join(root, "SHA256SUMS"), "utf8"), /[a-f0-9]{64} {2}latest\.json/);
  } finally {
    rmSync(root, { recursive: true });
  }
});

test("missing signatures, missing targets and non-HTTPS releases fail closed", () => {
  const root = fixture();
  try {
    assert.throws(() => createManifest(root, "1.1.0", "http://example.com"), /HTTPS/);
    const signature = join(root, "linux-x86_64-Hermes_1.1.0_amd64.AppImage.sig");
    writeFileSync(signature, "");
    assert.throws(() => createManifest(root, "1.1.0", "https://example.com"), /Empty/);
    rmSync(signature);
    assert.throws(() => createManifest(root, "1.1.0", "https://example.com"));
    rmSync(join(root, "linux-x86_64-Hermes_1.1.0_amd64.AppImage"));
    assert.throws(() => createManifest(root, "1.1.0", "https://example.com"), /linux-x86_64/);
  } finally {
    rmSync(root, { recursive: true });
  }
});

test("collection normalizes asset names and rejects collisions", () => {
  const root = mkdtempSync(join(tmpdir(), "hacc-bundles-"));
  const output = mkdtempSync(join(tmpdir(), "hacc-collected-"));
  try {
    writeFileSync(join(root, "Hermes Control Center.msi"), "installer");
    collectArtifacts(root, output, "windows-x86_64");
    assert.equal(
      readFileSync(join(output, "windows-x86_64-Hermes_Control_Center.msi"), "utf8"),
      "installer",
    );
    assert.throws(() => collectArtifacts(root, output, "windows-x86_64"), /Duplicate/);
  } finally {
    rmSync(root, { recursive: true });
    rmSync(output, { recursive: true });
  }
});
