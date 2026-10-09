// Dev-only: lets `pnpm dev` render in a plain browser for UI work. Never bundled in `tauri build`
// (main.tsx imports it only when import.meta.env.DEV and not inside Tauri).
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import catalog from "../src-tauri/catalog.json";

mockWindows("main");
document.documentElement.classList.add("no-mica"); // a browser has no Mica behind the page
const settings = { lang: "en", theme: "system", drive_dest: "C:\\Users\\you\\Downloads\\SetupHub-Drive", keep_installers: false, parallel_downloads: 3, catalog_url: "", clean_driver_install: true };

const fake = (id: string, interactive = false) => {
  let p = 0;
  const tick = setInterval(() => {
    p += 9;
    if (p < 100) emit("job", { id, phase: "downloading", progress: p });
    else {
      clearInterval(tick);
      emit("job", { id, phase: "installing", message: interactive ? "needs-interaction" : null });
      setTimeout(() => emit("job", id === "figma" ? { id, phase: "failed", message: "installer exited with code 1603 (0x00000643)" } : { id, phase: "done", reboot: id === "nordvpn", version: "1.0" }), 1400);
    }
  }, 250);
};

mockIPC(
  (cmd, args: any) => {
    switch (cmd) {
      case "bootstrap":
        return { items: catalog.items, catalog_origin: "embedded", settings, has_github_token: false, has_google_key: false, os_build: 26300, version: "1.0.0-dev" };
      case "detect_installed":
        return { zoom: "7.1.9", steam: "2.10.91.91", discord: "1.0.9261", claude: "2.31226.0.0" };
      case "install":
        args.ids.forEach((id: string) => (emit("job", { id, phase: "queued" }), setTimeout(() => fake(id, catalog.items.find((i) => i.id === id)?.interactive), 300)));
        return;
      case "reboot_pending":
        return ["NordVPN"];
      case "tweak_states":
        return [
          { id: "mouse_speed", applied: false, can_revert: false, detail: "6/11" },
          { id: "mouse_precision", applied: true, can_revert: true, detail: "off" },
          { id: "power_plan", applied: false, can_revert: false, detail: "Balanced" },
          { id: "hibernate", applied: false, can_revert: false, detail: "on" },
          { id: "fast_startup", applied: false, can_revert: false, detail: "on" },
        ];
      case "gpu_info":
        return {
          gpus: [{ name: "NVIDIA GeForce RTX 4080 SUPER", vendor: "Nvidia", driver_version: "32.0.16.1742", display_version: "617.42" }],
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
      case "ad_state":
        return { installed: false, discord: true, dotnet: true, vencord_cli: false, path: "" };
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
