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
  defaults?: { app: string; types: string[]; what: I18n } | null;
}
export interface Settings {
  lang: "en" | "ru";
  theme: "system" | "light" | "dark";
  drive_dest: string;
  keep_installers: boolean;
  parallel_downloads: number;
  catalog_url: string;
  clean_driver_install: boolean;
  allow_http: boolean;
  my_apps_url: string;
  check_updates: boolean;
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
export type Phase = "idle" | "queued" | "resolving" | "downloading" | "verifying" | "installing" | "review" | "done" | "failed" | "cancelled";

// ---- My Apps ----
export type Mode = "silent" | "download_only" | "interactive" | "extract";
export type MySource =
  | { type: "url"; url: string; hosts: string[] }
  | { type: "github"; repo: string; asset_regex: string }
  | { type: "winget"; id: string; store: "winget" | "msstore" };
export interface MyApp {
  id: string;
  name: string;
  icon?: string | null;
  publisher?: string | null;
  source: MySource;
  mode: Mode;
  silent_args: string[];
  detect: { display_name?: string | null; path?: string | null; appx?: string | null; uninstall_key?: string | null };
  include_in_install_all: boolean;
  added_at: number;
  extract_to?: string | null;
  shortcut?: string | null;
  size?: number | null;
  version?: string | null;
  file_type?: string | null;
  installer_type?: string | null;
  signature?: string | null;
  signer?: string | null;
  reviewed: boolean;
  allow_unsigned: boolean;
}
export interface WingetMatch { name: string; id: string; version: string }
export type Analysis =
  | { kind: "ready"; input: string; entry: MyApp }
  | { kind: "winget_choices"; input: string; matches: WingetMatch[] }
  | { kind: "page_links"; input: string; page: string; links: string[] }
  | { kind: "drive"; input: string; file?: string | null; folder?: string | null; name?: string | null }
  | { kind: "error"; input: string; message: string; hint?: string | null; open_url?: string | null };
export const sourceLabel = (s: MySource) =>
  s.type === "url" ? s.url : s.type === "github" ? `github.com/${s.repo}` : s.store === "msstore" ? `Microsoft Store · ${s.id}` : `winget · ${s.id}`;
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
  checkSelfUpdate: () => invoke<SelfUpdate | null>("check_self_update"),
  applySelfUpdate: () => invoke<void>("apply_self_update"),
  checkAppUpdates: () => invoke<AppUpdate[]>("check_app_updates"),
  defaultsStates: () => invoke<DefaultState[]>("defaults_states"),
  defaultsApply: (id: string) => invoke<"next_sign_in" | "next_sign_in_home" | "use_settings">("defaults_apply", { id }),
  defaultsRevert: (id: string) => invoke<void>("defaults_revert", { id }),
  defaultsSettingsUri: (id: string) => invoke<string>("defaults_settings_uri", { id }),
  gpuInfo: () => invoke<GpuInfo>("gpu_info"),
  installNvidia: (clean: boolean) => invoke<void>("install_nvidia", { clean }),
  driveList: (folder?: string) => invoke<DriveEntry[]>("drive_list", { folder: folder ?? null }),
  myappsList: () => invoke<MyApp[]>("myapps_list"),
  myappsAnalyze: (input: string) => invoke<Analysis[]>("myapps_analyze", { input }),
  myappsAdd: (entries: MyApp[]) => invoke<MyApp[]>("myapps_add", { entries }),
  myappsUpdate: (entry: MyApp) => invoke<void>("myapps_update", { entry }),
  myappsRemove: (id: string) => invoke<void>("myapps_remove", { id }),
  myappsDuplicate: (id: string) => invoke<void>("myapps_duplicate", { id }),
  myappsReorder: (ids: string[]) => invoke<void>("myapps_reorder", { ids }),
  myappsCheck: () => invoke<Record<string, { error?: string | null; latest?: string | null }>>("myapps_check"),
  myappsUninstall: (id: string) => invoke<void>("myapps_uninstall", { id }),
  myappsExport: (path: string) => invoke<void>("myapps_export", { path }),
  myappsImport: (path: string) => invoke<MyApp[]>("myapps_import", { path }),
  myappsFetchUrl: (url: string) => invoke<MyApp[]>("myapps_fetch_url", { url }),
  logTail: (id: string) => invoke<string>("log_tail", { id }),
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

export interface SelfUpdate { version: string; url: string; sha256?: string | null; notes_url: string }
export interface AppUpdate { id: string; installed: string; available: string }

/** One card per catalog app that replaces a built-in Windows app; id is the catalog item id. */
export interface DefaultState {
  id: string;
  name: string;
  installed: boolean;
  applied: boolean;
  pending: boolean;
  can_revert: boolean;
  ours: number;
  total: number;
  home: boolean;
  settings_uri: string;
}
export interface TweakState { id: string; applied?: boolean; can_revert?: boolean; detail?: string; error?: string }
export interface Gpu { name: string; vendor: "Nvidia" | "Amd" | "Intel" | "Other"; driver_version: string; display_version?: string | null; driver_missing: boolean }
export interface DriverInfo { version: string; release_date: string; url: string; size: string; name: string }
export interface GpuInfo {
  gpus: Gpu[];
  nvidia_latest?: DriverInfo | null;
  nvidia_error?: string | null;
  update_available: boolean;
  detect_error?: string | null;
  use_nvidia_app: boolean;
}
export interface DriveEntry { id: string; name: string; mime: string; is_folder: boolean; size?: number | null; path: string }
export interface AdState { installed: boolean; discord: boolean; dotnet: boolean; vencord_cli: boolean; path: string }
export interface License { name: string; description: string; status: number; partial_key: string; grace_minutes: number }

/** "1.10.0" > "1.9.1" → true. Non-numeric parts compare as 0. */
export const isNewer = (a: string, b: string) => {
  const p = (s: string) => s.replace(/^v/i, "").split(/[.\-_ ]/).map((x) => parseInt(x, 10) || 0);
  const [x, y] = [p(a), p(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  return false;
};

export const fmtSize = (b?: number | null) => {
  if (b == null) return "—";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
  return `${b.toFixed(i ? 1 : 0)} ${u[i]}`;
};
