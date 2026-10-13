import { describe, expect, it } from "vitest";
import { maskHermesConfig, restoreHermesConfigSecrets } from "./hermesConfigDraft";

describe("Hermes config secret masking", () => {
  it("masks nested secrets and restores unchanged values by YAML path", () => {
    const source = `provider:\n  api_key: original-key\n  nested:\n    accessToken: original-token\nmodels:\n  - name: local\n    password: array-secret\n`;
    const masked = maskHermesConfig(source);

    expect(masked.source).not.toContain("original-key");
    expect(masked.source).not.toContain("original-token");
    expect(masked.source).not.toContain("array-secret");
    expect(masked.source.match(/\*{8}/g)).toHaveLength(3);
    expect(restoreHermesConfigSecrets(masked.source, masked.secrets)).toContain(
      'api_key: "original-key"',
    );
    expect(restoreHermesConfigSecrets(masked.source, masked.secrets)).toContain(
      'accessToken: "original-token"',
    );
  });

  it("masks string secrets nested under secret-named arrays", () => {
    const masked = maskHermesConfig("api_keys:\n  - first-secret\n  - second-secret\n");

    expect(masked.source).not.toContain("first-secret");
    expect(masked.source).not.toContain("second-secret");
    expect(masked.source.match(/\*{8}/g)).toHaveLength(2);
    expect(restoreHermesConfigSecrets(masked.source, masked.secrets)).toContain("first-secret");
  });

  it("keeps edited secret values and allows deleting a masked field", () => {
    const masked = maskHermesConfig("api_key: original-key\npassword: remove-me\n");
    const edited = masked.source.replace('api_key: "********"', 'api_key: "replacement-key"');
    const withoutPassword = edited.replace(/^password:.*\n/m, "");
    const restored = restoreHermesConfigSecrets(withoutPassword, masked.secrets);

    expect(restored).toContain("replacement-key");
    expect(restored).not.toContain("original-key");
    expect(restored).not.toContain("remove-me");
  });

  it("rejects malformed YAML and non-mapping roots", () => {
    expect(() => maskHermesConfig("api_key: [")).toThrow("invalid YAML");
    expect(() => maskHermesConfig("- item\n")).toThrow("mapping at its root");
  });
});
