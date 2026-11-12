import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { BotTokens, Sample, Settings, SettingsView, Target, TargetStatus, TargetView } from "./types";

const STATUS_EVENT = "target://status";
const SETTINGS_EVENT = "settings://changed";

export const listTargets = () => invoke<TargetView[]>("list_targets");

export const listStatuses = () => invoke<TargetStatus[]>("list_statuses");

export const saveTarget = (target: Target, secret?: string) =>
  invoke<Target>("save_target", { target, secret: secret ?? null });

export const deleteTarget = (id: string) => invoke<void>("delete_target", { id });

export const setTargetEnabled = (id: string, enabled: boolean) =>
  invoke<void>("set_target_enabled", { id, enabled });

export const snoozeTarget = (id: string, until: number | null) =>
  invoke<void>("snooze_target", { id, until });

export const snoozeAlerts = (until: number | null) => invoke<void>("snooze_alerts", { until });

export const onSettingsChanged = (handler: () => void) =>
  listen<number | null>(SETTINGS_EVENT, () => handler());

export const checkNow = (id: string) => invoke<TargetStatus>("check_now", { id });

export const getHistory = (id: string, since: number) =>
  invoke<Sample[]>("get_history", { id, since });

export const getSettings = () => invoke<SettingsView>("get_settings");

export const saveSettings = (settings: Settings, tokens: BotTokens) =>
  invoke<Settings>("save_settings", {
    settings,
    telegramToken: tokens.telegram ?? null,
    baleToken: tokens.bale ?? null,
  });

export const sendTestNotification = () => invoke<void>("send_test_notification");

export const onStatus = (handler: (status: TargetStatus) => void) =>
  listen<TargetStatus>(STATUS_EVENT, (event) => handler(event.payload));

export function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
