import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  isPermissionGranted: vi.fn<() => Promise<boolean>>(),
  requestPermission: vi.fn<() => Promise<NotificationPermission>>(),
  sendNotification: vi.fn(),
  isVisible: vi.fn<() => Promise<boolean>>(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ isVisible: mocks.isVisible }),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: mocks.isPermissionGranted,
  requestPermission: mocks.requestPermission,
  sendNotification: mocks.sendNotification,
}));

import { notifyWhenWindowHidden } from "./nativeNotifications";

describe("notifyWhenWindowHidden", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.isVisible.mockResolvedValue(false);
    mocks.isPermissionGranted.mockResolvedValue(true);
    mocks.requestPermission.mockResolvedValue("granted");
  });

  it("does not send a native notification while the window is visible", async () => {
    mocks.isVisible.mockResolvedValue(true);

    await expect(notifyWhenWindowHidden("Hermes stopped", "Phone A")).resolves.toBe(false);
    expect(mocks.isPermissionGranted).not.toHaveBeenCalled();
    expect(mocks.sendNotification).not.toHaveBeenCalled();
  });

  it("sends a native notification when hidden and already authorized", async () => {
    await expect(notifyWhenWindowHidden("Hermes stopped", "Phone A")).resolves.toBe(true);
    expect(mocks.requestPermission).not.toHaveBeenCalled();
    expect(mocks.sendNotification).toHaveBeenCalledWith({
      title: "Hermes stopped",
      body: "Phone A",
    });
  });

  it("requests permission before sending when hidden", async () => {
    mocks.isPermissionGranted.mockResolvedValue(false);

    await notifyWhenWindowHidden("Hermes stopped", "Phone A");
    expect(mocks.requestPermission).toHaveBeenCalledOnce();
    expect(mocks.sendNotification).toHaveBeenCalledOnce();
  });

  it("does not send a native notification when permission is denied", async () => {
    mocks.isPermissionGranted.mockResolvedValue(false);
    mocks.requestPermission.mockResolvedValue("denied");

    await expect(notifyWhenWindowHidden("Hermes stopped", "Phone A")).resolves.toBe(false);
    expect(mocks.sendNotification).not.toHaveBeenCalled();
  });
});
