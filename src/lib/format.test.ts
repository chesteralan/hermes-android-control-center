import { describe, expect, it } from "vitest";
import { formatBytes, formatSeconds, orUnknown, validateHost, validatePort } from "./format";

describe("format", () => {
  it("formats bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(48 * 1024 ** 3)).toBe("48 GB");
    expect(formatBytes(-1)).toBe("Unknown");
  });

  it("formats seconds", () => {
    expect(formatSeconds(5000)).toBe("5s");
    expect(formatSeconds(90000)).toBe("1m 30s");
  });

  it("never fabricates values", () => {
    expect(orUnknown(null)).toBe("Unknown");
    expect(orUnknown("")).toBe("Unknown");
    expect(orUnknown(0)).toBe("0");
  });

  it("validates host and port like the backend", () => {
    expect(validateHost("192.168.1.25")).toBeNull();
    expect(validateHost("pixel.local")).toBeNull();
    expect(validateHost("999.1.1.1")).not.toBeNull();
    expect(validateHost("a;b")).not.toBeNull();
    expect(validatePort("5555")).toBeNull();
    expect(validatePort("0")).not.toBeNull();
    expect(validatePort("70000")).not.toBeNull();
  });
});
