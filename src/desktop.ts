import { invoke } from "@tauri-apps/api/core";
import { desktop } from "./api";
export interface DesktopPreferences { start_with_windows: boolean; close_to_tray: boolean }
export interface DesktopStatus extends DesktopPreferences { startup_supported: boolean; tray_available: boolean; error: string | null }
function previewSettings(): DesktopStatus {
  const saved = localStorage.getItem("challa-preview-desktop");
  return { start_with_windows: false, close_to_tray: true, ...(saved ? JSON.parse(saved) : {}), startup_supported: false, tray_available: false, error: null };
}
export async function desktopSettings(): Promise<DesktopStatus> { return desktop ? invoke("get_desktop_settings") : previewSettings(); }
export async function saveDesktopSettings(preferences: DesktopPreferences): Promise<DesktopStatus> {
  if (desktop) return invoke("save_desktop_settings", { preferences });
  localStorage.setItem("challa-preview-desktop", JSON.stringify(preferences));
  return previewSettings();
}
export async function hideToTray(): Promise<void> { if (desktop) await invoke("hide_to_tray"); }
