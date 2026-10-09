import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";

export type I18n = { en: string; ru: string };
export interface Item {
  id: string;
  name: string;
  category: string;
  description: I18n;
  winget_id?: string;
  source: { type: string; url?: string; repo?: string };
  interactive: boolean;
  unelevated: boolean;
  needs_reboot: boolean;
  note?: I18n | null;
}
export interface Settings {
  lang: "en" | "ru";
  theme: "system" | "light" | "dark";
  drive_dest: string;
  keep_installers: boolean;
  parallel_downloads: number;
  catalog_url: string;
  clean_driver_install: boolean;
}
export interface Boot {
  items: Item[];
  catalog_origin: string;
  settings: Settings;
  has_github_token: boolean;
  has_google_key: boolean;
  os_build: number;
  version: string;
}
export type Phase = "idle" | "queued" | "resolving" | "downloading" | "verifying" | "installing" | "done" | "failed" | "cancelled";
export interface Job {
  id: string;
  phase: Phase;
  progress?: number | null;
  message?: string | null;
  reboot?: boolean;
  version?: string | null;
}
export const ACTIVE: Phase[] = ["queued", "resolving", "downloading", "verifying", "installing"];

// ---- job store: one Tauri event stream feeds every card ----
const jobs = new Map<string, Job>();
export interface LogLine { t: Date; id: string; phase: Phase; message?: string | null }
const log: LogLine[] = [];
const subs = new Set<() => void>();
let version = 0;
const notify = () => { version++; subs.forEach((f) => f()); };

let listening = false;
function ensureListening() {
  if (listening) return;
  listening = true;
  listen<Job>("job", ({ payload }) => {
    const prev = jobs.get(payload.id);
    // Progress ticks carry no message; keep the last one (e.g. "needs-interaction") visible.
    jobs.set(payload.id, { ...payload, message: payload.message ?? (prev?.phase === payload.phase ? prev?.message : null) });
    if (prev?.phase !== payload.phase) log.push({ t: new Date(), id: payload.id, phase: payload.phase, message: payload.message });
    notify();
  });
}

const subscribe = (f: () => void) => { ensureListening(); subs.add(f); return () => subs.delete(f); };
export const useJob = (id: string): Job | undefined => useSyncExternalStore(subscribe, () => jobs.get(id));
export const useJobsVersion = () => useSyncExternalStore(subscribe, () => version);
export const getJob = (id: string) => jobs.get(id);
export const getLog = () => log;
export const isActive = (j?: Job) => !!j && ACTIVE.includes(j.phase);

// ---- commands ----
export const api = {
  bootstrap: () => invoke<Boot>("bootstrap"),
  detectInstalled: () => invoke<Record<string, string>>("detect_installed"),
  reloadCatalog: (url: string) => invoke<[Item[], string]>("reload_catalog", { url }),
  install: (ids: string[]) => invoke<void>("install", { ids }),
  cancel: (id: string) => invoke<void>("cancel", { id }),
  rebootPending: () => invoke<string[]>("reboot_pending"),
  restartNow: () => invoke<void>("restart_now"),
  tweakStates: () => invoke<TweakState[]>("tweak_states"),
  tweakApply: (id: string) => invoke<void>("tweak_apply", { id }),
  tweakRevert: (id: string) => invoke<void>("tweak_revert", { id }),
  gpuInfo: () => invoke<GpuInfo>("gpu_info"),
  installNvidia: (clean: boolean) => invoke<void>("install_nvidia", { clean }),
  driveList: () => invoke<DriveEntry[]>("drive_list"),
  driveDownload: (files: { id: string; name: string; path: string }[], dest: string) => invoke<void>("drive_download", { files, dest }),
  adState: () => invoke<AdState>("ad_state"),
  adInstall: () => invoke<void>("ad_install"),
  licenseStatus: () => invoke<License[]>("license_status"),
  activateKey: (key: string) => invoke<string>("activate_key", { key }),
  saveSettings: (s: Settings) => invoke<void>("save_settings", { s }),
  setSecret: (name: "github_token" | "google_api_key", value: string) => invoke<void>("set_secret", { name, value }),
  logsDir: () => invoke<string>("logs_dir"),
  exportLog: () => invoke<string>("export_log"),
};

export interface TweakState { id: string; applied?: boolean; can_revert?: boolean; detail?: string; error?: string }
export interface Gpu { name: string; vendor: "Nvidia" | "Amd" | "Intel" | "Other"; driver_version: string; display_version?: string | null }
export interface DriverInfo { version: string; release_date: string; url: string; size: string; name: string }
export interface GpuInfo { gpus: Gpu[]; nvidia_latest?: DriverInfo | null; nvidia_error?: string | null; update_available: boolean }
export interface DriveEntry { id: string; name: string; mime: string; is_folder: boolean; size?: number | null; path: string }
export interface AdState { installed: boolean; discord: boolean; dotnet: boolean; vencord_cli: boolean; path: string }
export interface License { name: string; description: string; status: number; partial_key: string; grace_minutes: number }

export const fmtSize = (b?: number | null) => {
  if (b == null) return "—";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
  return `${b.toFixed(i ? 1 : 0)} ${u[i]}`;
};
