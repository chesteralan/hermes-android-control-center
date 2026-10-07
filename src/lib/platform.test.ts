import { describe, expect, it } from "vitest";
import { modifierLabel, monospaceFontFor } from "./platform";

describe("desktop platform presentation", () => {
  it("uses Command on macOS and Ctrl on Windows/Linux", () => {
    expect(modifierLabel("macos")).toBe("⌘");
    expect(modifierLabel("windows")).toBe("Ctrl+");
    expect(modifierLabel("linux")).toBe("Ctrl+");
  });
  it("uses native monospace fonts with a generic fallback", () => {
    expect(monospaceFontFor("macos")).toContain("SF Mono");
    expect(monospaceFontFor("windows")).toContain("Consolas");
    expect(monospaceFontFor("linux")).toContain("DejaVu Sans Mono");
  });
});
