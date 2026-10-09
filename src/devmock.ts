// Dev-only: lets `pnpm dev` render in a plain browser for UI work. Never bundled in `tauri build`
// (main.tsx imports it only when import.meta.env.DEV and not inside Tauri).
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import catalog from "../src-tauri/catalog.json";

mockWindows("main");
document.documentElement.classList.add("no-mica"); // a browser has no Mica behind the page
const settings = { lang: "en", theme: "system", drive_dest: "C:\\Users\\you\\Downloads\\SetupHub-Drive", keep_installers: false, parallel_downloads: 3, catalog_url: "", clean_driver_install: true, allow_http: false, my_apps_url: "", check_updates: true };

const fake = (id: string, interactive = false) => {
  let p = 0;
  const tick = setInterval(() => {
    p += 9;
    if (p < 100) emit("job", { id, phase: "downloading", progress: p });
    else {
      clearInterval(tick);
      emit("job", { id, phase: "installing", message: interactive ? "needs-interaction" : null });
      const message = ["vlc", "chrome", "pdfcraft"].includes(id) ? "default-signin" : id === "flameshot" ? "default-print-screen" : null;
      setTimeout(() => emit("job", id === "figma" ? { id, phase: "failed", message: "installer exited with code 1603 (0x00000643)" } : { id, phase: "done", reboot: id === "nordvpn", version: "1.0", message }), 1400);
    }
  }, 250);
};

const base = { detect: {}, include_in_install_all: true, added_at: 0, reviewed: false, allow_unsigned: false, silent_args: [] as string[], mode: "silent" };
let myApps: any[] = [
  { ...base, id: "my-viber", name: "Viber", publisher: "viber.com", source: { type: "url", url: "https://download.cdn.viber.com/desktop/windows/ViberSetup.msi", hosts: [] }, silent_args: ["/qn", "/norestart"], file_type: "msi", size: 149270528 },
  { ...base, id: "my-nanazip", name: "NanaZip", publisher: "M2Team", icon: "https://github.com/M2Team.png?size=96", source: { type: "github", repo: "M2Team/NanaZip", asset_regex: "^NanaZip_[0-9][0-9._]*\\.msixbundle$" }, file_type: "msixbundle", version: "7.0.1843.0" },
  { ...base, id: "my-tool", name: "Portable Tool", publisher: "example.com", source: { type: "url", url: "https://example.com/tool.zip", hosts: [] }, mode: "extract", extract_to: "%LOCALAPPDATA%\\Programs\\Portable Tool", include_in_install_all: false, installer_type: "zip", signature: "Archive" },
  { ...base, id: "my-unsigned", name: "Unsigned App", publisher: "example.org", source: { type: "url", url: "https://example.org/app-setup.exe", hosts: [] }, installer_type: "nsis", signature: "NotSigned", silent_args: ["/S"] },
];
const pushMy = () => emit("myapps", myApps);

mockIPC(
  (cmd, args: any) => {
    switch (cmd) {
      case "myapps_list":
        return myApps;
      case "myapps_analyze":
        return [
          { kind: "ready", input: "https://github.com/M2Team/NanaZip", entry: { ...base, id: "", name: "NanaZip", publisher: "M2Team", icon: "https://github.com/M2Team.png?size=96", source: { type: "github", repo: "M2Team/NanaZip", asset_regex: "x" }, file_type: "msixbundle", size: 11931446, version: "7.0.1843.0" } },
          { kind: "winget_choices", input: "obs studio", matches: [{ name: "OBS Studio", id: "OBSProject.OBSStudio", version: "32.2.2" }, { name: "OBS Studio Beta", id: "OBSProject.OBSStudio.Pre-release", version: "32.0.0-rc1" }] },
          { kind: "error", input: "https://example.com/run.ps1", message: "Scripts (.bat, .cmd, .ps1, .vbs, .js, .hta, .reg) are never run from links", hint: "Download it yourself and inspect it before running" },
        ].slice(0, args.input.split("\n").filter((l: string) => l.trim()).length || 1);
      case "myapps_add":
        myApps = [...myApps, ...args.entries.map((e: any, i: number) => ({ ...e, id: e.id || `my-new-${myApps.length + i}` }))];
        pushMy();
        return myApps;
      case "myapps_update":
        myApps = myApps.map((a) => (a.id === args.entry.id ? args.entry : a));
        pushMy();
        return;
      case "myapps_remove":
        myApps = myApps.filter((a) => a.id !== args.id);
        pushMy();
        return;
      case "myapps_reorder":
        myApps = args.ids.map((id: string) => myApps.find((a) => a.id === id));
        pushMy();
        return;
      case "myapps_check":
        return { "my-viber": {}, "my-nanazip": { latest: "7.1.0.0" }, "my-tool": { error: "HTTP status 404 Not Found" } };
      case "myapps_import":
      case "myapps_fetch_url":
        return myApps.slice(0, 2).map((a) => ({ ...a, id: a.id + "-imp" }));
      case "bootstrap":
        return { items: catalog.items, catalog_origin: "embedded", settings, has_github_token: false, has_google_key: false, os_build: 26300, version: "1.0.0-dev" };
      case "detect_installed":
        return { zoom: "7.1.9", steam: "2.10.91.91", discord: "1.0.9261", claude: "2.31226.0.0", "my-nanazip": "7.0.1843.0" };
      case "install":
        args.ids.forEach((id: string) => {
          emit("job", { id, phase: "queued" });
          const m = myApps.find((a) => a.id === id);
          // My Apps: the first download stops at "review".
          if (m && !m.reviewed) setTimeout(() => emit("job", { id, phase: "review" }), 900);
          else setTimeout(() => fake(id, catalog.items.find((i) => i.id === id)?.interactive), 300);
        });
        return;
      case "reboot_pending":
        return ["NordVPN"];
      case "tweak_states":
        return [
          { id: "mouse_speed", applied: false, can_revert: false, detail: "6/11" },
          { id: "mouse_precision", applied: true, can_revert: true, detail: "off" },
          { id: "power_plan", applied: false, can_revert: false, detail: "Balanced" },
          { id: "print_screen", applied: false, can_revert: false, detail: "on" },
          { id: "hibernate", applied: false, can_revert: false, detail: "on" },
          { id: "fast_startup", applied: false, can_revert: false, detail: "on" },
        ];
      case "defaults_states":
        return [
          { id: "chrome", name: "Google Chrome", installed: true, applied: true, pending: false, can_revert: true, ours: 7, total: 7, home: false, settings_uri: "ms-settings:defaultapps?registeredAppMachine=Google%20Chrome" },
          { id: "vlc", name: "VLC media player", installed: true, applied: false, pending: true, can_revert: true, ours: 0, total: 114, home: false, settings_uri: "ms-settings:defaultapps?registeredAppMachine=VLC" },
          { id: "pdfcraft", name: "PdfCraft", installed: false, applied: false, pending: false, can_revert: false, ours: 0, total: 1, home: false, settings_uri: "ms-settings:defaultapps" },
        ];
      case "check_self_update":
        return { version: "1.3.0", url: "https://github.com/reyjendiz/setup-hub/releases/download/v1.3.0/setup-hub.exe", sha256: null, notes_url: "" };
      case "check_app_updates":
        return [{ id: "steam", installed: "2.10.91.91", available: "2.11.0.4" }];
      case "defaults_apply":
        return "next_sign_in";
      case "defaults_settings_uri":
        return "ms-settings:defaultapps";
      case "gpu_info":
        return {
          gpus: [{ name: "NVIDIA GeForce RTX 4080 SUPER", vendor: "Nvidia", driver_version: "32.0.16.1742", display_version: "617.42", driver_missing: false }],
          use_nvidia_app: false,
          nvidia_latest: { version: "617.42", release_date: "Tue Oct 06, 2026", url: "", size: "990.85 MB", name: "GeForce Game Ready Driver" },
          update_available: false,
        };
      case "drive_list":
        return [
          { id: "a", name: "4K.Video.Downloader.exe", mime: "application/x-msdownload", is_folder: false, size: 123456789, path: "" },
          { id: "b", name: "mods.zip", mime: "application/zip", is_folder: false, size: 9876543, path: "" },
          { id: "c", name: "Presets", mime: "folder", is_folder: true, size: null, path: "" },
          { id: "d", name: "preset-1.json", mime: "application/json", is_folder: false, size: 2048, path: "Presets" },
        ];
      case "drive_download":
        args.files.forEach((f: any) => fake(`drive:${f.id}`));
        return;
      case "ven_state":
        return { installed: true, autostart: true, discord: true, vencord: true, last_run: { time: Date.now() / 1000 - 3600, vencord_ok: true, discord_started: true, message: "Vencord is up to date; Discord started" }, path: "C:\\Users\\you\\AppData\\Local\\Programs\\Ven\\Ven.exe" };
      case "license_status":
        return [{ name: "Windows(R), Professional edition", description: "Windows(R) Operating System, RETAIL channel", status: 1, partial_key: "3V66T", grace_minutes: 0 }];
      case "logs_dir":
        return "C:\\logs";
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);
