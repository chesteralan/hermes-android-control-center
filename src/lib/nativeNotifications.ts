import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export async function notifyWhenWindowHidden(title: string, body: string): Promise<boolean> {
  try {
    if (await getCurrentWindow().isVisible()) return false;

    let permissionGranted = await isPermissionGranted();
    if (!permissionGranted) permissionGranted = (await requestPermission()) === "granted";
    if (!permissionGranted) return false;

    sendNotification({ title, body });
    return true;
  } catch (error) {
    console.warn("Could not send native notification", error);
    return false;
  }
}
