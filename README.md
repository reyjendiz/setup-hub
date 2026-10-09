# Setup Hub

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshot-dark.png">
  <img alt="Setup Hub — Apps page" src="docs/screenshot.png">
</picture>

**[⬇ Download the latest release](https://github.com/reyjendiz/setup-hub/releases/latest)** — `setup-hub.exe`, portable, nothing to install.

Everything you set up after reinstalling Windows 10/11 x64, from one file. Pick your apps, press **Install all**, and Setup Hub downloads, verifies and installs them silently, one after another — then gets out of the way.

### What it does

- **Apps** — 77 free apps in two tabs. **Programs** is grouped into *Everyday* (browsers, messengers, media, notes, system utilities), *For designers* (Blender, Affinity, GIMP, Krita, the ArtCraft apps…), *For programmers* (VS Code, Cursor, Docker, Postman…) and *Dev toolchain* (Git, Node.js, Python, uv, FFmpeg); **Games** has the launchers. Each group installs in one click; winget first, the vendor's signed installer as fallback, with live progress, cancel, retry and one restart prompt at the end.
- **Updates** — Setup Hub updates itself from GitHub Releases (*Update and restart*), and finds newer versions of the apps it installed (*Update all*).
- **Default apps** — apps that replace a built-in one become the default: Chrome for links, VLC for video and music, PdfCraft for PDFs, Flameshot on the Print Screen key. Every change can be reverted in **Tweaks**.
- **Drivers** — reads your graphics card from Windows and installs the latest NVIDIA Game Ready driver; if Windows doesn't know the model yet (fresh install), it installs the NVIDIA App, which finds it. AMD and Intel get their official pages.
- **Tweaks** — pointer speed, mouse acceleration off, High performance power plan, Print Screen without Snipping Tool, hibernation and Fast Startup — one click each, each one reverts.
- **Ven** — keeps [Vencord](https://github.com/Vendicated/Vencord) in Discord: at every sign-in it installs the latest Vencord, then opens Discord. No .NET, no admin rights.
- **My Apps** — add any program by pasting a link (GitHub repo, direct download, winget ID, Microsoft Store, web page); it installs like the built-in ones, after you review its signature once.
- **Files** — downloads your shared Google Drive folder, no sign-in.
- **Activation** — Windows license status and *Enter my own key*. No KMS, no third-party keys.
- **Settings** — English / Russian, light / dark theme, download folder, parallel downloads, tokens in Windows Credential Manager, remote catalog, update checks, log export.

Safe by default: HTTPS only, a host allow-list per app, SHA-256 and Authenticode checks before anything runs, scripts are never executed, no telemetry.

## Build

Prerequisites: Node 20+, pnpm, Rust (stable, MSVC), Visual Studio Build Tools with the C++ workload. WebView2 is already on Windows 11.

```bash
pnpm i
pnpm tauri build
```

Outputs:
- `src-tauri/target/release/setup-hub.exe` — the portable single exe (frontend, catalog and Ven are embedded; requests admin via manifest).
- `src-tauri/target/release/bundle/nsis/Setup Hub_1.2.0_x64-setup.exe` — optional installer.

Other commands:

| Command | What it does |
|---|---|
| `pnpm dev` | UI only in a browser with mocked backend (`src/devmock.ts`, dev builds only) |
| `pnpm tauri dev` | Real app, **not** elevated (dev manifest is `asInvoker`); run the terminal as admin to test installs/tweaks |
| `pnpm test` | Rust unit tests (catalog, asset regex, Drive parsing, GPU vendor/version, mouse/power logic, exit codes, My Apps link/installer-type detection, schema validation, import/export…) |
| `cd src-tauri && cargo test --lib live -- --ignored --nocapture` | Read-only live checks against this PC and the real endpoints |
| `pnpm verify-catalog` | Resolves every catalog link, downloads non-winget installers, checks hash/signature, writes `catalog-report.md` |
| `pnpm icons` | Re-fetches app icons into `public/icons` (build time only; the app makes no icon requests) |

CI: `.github/workflows/build.yml` builds on `windows-latest`, runs the unit tests and uploads both executables. Code signing is not configured — add your certificate via `bundle.windows.certificateThumbprint` or a `signtool` step.

## How installs work

```
resolve (winget → direct URL / vendor page regex / GitHub Releases API)
  → download (HTTPS only, per-item host allow-list incl. redirects, HTTP Range resume, 3 retries with back-off, parallel 1–4)
  → verify (SHA-256 when published — GitHub asset digests — and Authenticode: must be Valid and match the expected publisher;
            unsigned is accepted only when the hash was verified)
  → install (one at a time; MSI via msiexec; ZIPs extracted with zip-slip protection)
  → detect (uninstall registry HKLM/HKLM-WOW/HKCU, MSIX package repository, or a known path)
```

Exit codes 3010/1641 (and winget's "reboot required") collect into one *Restart now* banner. Logs rotate daily in `%LOCALAPPDATA%\SetupHub\logs` (7 days). Downloads live in `%TEMP%\SetupHub\cache` and are deleted after a successful install unless *Keep installers* is on.

### Catalog

`src-tauri/catalog.json` is embedded. Set *Settings → Custom catalog URL* to a raw GitHub URL of your own copy to update links without rebuilding; the last good remote copy is cached and the embedded one is the offline fallback. A remote catalog is validated the same way (https only, URL host must be in that item's `allowed_hosts`).

Schema per item: `id, name, category, description{en,ru}, winget_id?, source{type: winget|direct|github_release|scrape, url|repo+asset_regex|page+regex}, silent_args[], detect{display_name?, path?, appx?}, allowed_hosts[], publisher?, zip?{run|extract_to+shortcut+launch}, needs_reboot, interactive, unelevated, note?, defaults?{app, types[], what{en,ru}}`. Category `gaming` goes to the **Games** tab; in **Programs**, `creative` → *For designers*, `coding` → *For programmers*, `dev` → *Dev toolchain*, everything else → *Everyday*.

See **[catalog-report.md](catalog-report.md)** for the verified URL, version, installer type, signer and silent switches of every item.

## Default apps

Catalog apps that replace a built-in Windows app are made the default as part of their install, and get a card under **Tweaks → Default apps** (state, **Apply**, **Revert**, **Open Settings**):

| App | Replaces | Types |
|---|---|---|
| Google Chrome | Edge as the browser | `http`, `https`, `.htm`, `.html`, `.shtml`, `.xht`, `.xhtml` |
| VLC media player | Media Player | 114 video, audio and playlist types |
| PdfCraft | Edge as the PDF reader | `.pdf` |

Windows protects each user's choice (UserChoice / UserChoiceLatest) with a hash that only Windows may write, so Setup Hub doesn't forge it. Instead it reads the ProgIds the app registered (`RegisteredApplications\VLC` → `Capabilities\FileAssociations` / `URLAssociations`, e.g. `.mkv` → `VLC.mkv`, `https` → `ChromeHTML`), writes them to `%ProgramData%\SetupHub\DefaultAssociations.xml` and points `HKLM\SOFTWARE\Policies\Microsoft\Windows\System\DefaultAssociationsConfiguration` at it. Windows applies it at the **next sign-in**. Every association is `Suggested="true"`, so on Windows 11 22H2+ it's applied once per `Version` (bumped on each Apply) and you can still pick another app later; Windows 10 re-applies it at every sign-in until you Revert. Only types the app itself registered *and* the catalog lists are used — VLC also registers `.iso`, `.zip`, `.rar` and skin files, Chrome `.pdf`, `mailto` and `ftp`; they don't get those. Revert removes the app from the file (and the policy when it's empty); files keep opening in it until you choose another app. An associations policy configured by someone else is never overwritten — the card then offers Settings instead. Microsoft documents the policy for Pro/Enterprise/Education; on Home the card says it may be ignored and offers **Open Settings** (the app's own page in Settings › Default apps on Windows 11).

Catalog schema: `defaults.app` (RegisteredApplications name), `defaults.types` (extensions, plus `http`/`https`; executables, scripts, `.lnk` and `.url` are rejected even from a remote catalog), `defaults.what` (EN/RU label for the card). My Apps entries can't carry defaults.

## Updates

- **Setup Hub** — on launch (Settings → *Check for updates on launch*, on by default) it reads the latest release of `reyjendiz/setup-hub`. A newer version shows *Update and restart*: it downloads the release's `setup-hub.exe`, verifies it against the SHA-256 digest GitHub records for the asset (or a valid signature), renames the running exe to `setup-hub.old.exe`, puts the new one in its place and restarts into it (the old file is deleted on the next start). The release is looked up again when you click, so nothing from the UI decides what gets installed. Releases come from CI: pushing a `v*` tag builds and publishes `setup-hub.exe` + the installer.
- **Apps** — `winget upgrade` (matched by package id, so any language works) for winget apps, the latest GitHub release vs. the installed version for apps installed from GitHub. Cards show `old → new` and **Update**; **Update all (n)** runs them through the normal install pipeline (winget upgrades in place, MSI/NSIS installers install over the old version). Apps from direct vendor links have no version to compare and are skipped.

## My Apps (add programs by link)

**+ Add** (title bar, or the My Apps page) opens a sheet: paste one or more lines — the clipboard is read only when you press **Paste** — and each line is analyzed:

| You paste | What happens |
|---|---|
| `github.com/owner/repo` | Releases API → best Windows x64 asset (`.msi` > `*setup*.exe` > `.msix` > `.exe` > `.zip` > `.7z`). The asset name is turned into a version-agnostic regex, so "latest" is re-resolved on every install and the card shows **Update …** after *Check links*. |
| Direct `.exe/.msi/.msix/.appx/.zip/.7z` link | HEAD (or 1-byte GET) for size and file name, following redirects; the domains seen are remembered as the entry's host allow-list. |
| Google Drive file/folder link | "Download to my files folder" through the existing Drive downloader. |
| winget ID / free text | `winget show` / `winget search` (up to 8 matches to pick from). |
| Microsoft Store link or ID (`9…`, `XP…`) | `winget install --source msstore`. |
| Web page | Lists Windows download links found on it to pick from; none → "Open page in browser". |
| `.bat .cmd .ps1 .vbs .js .hta .reg …` | Refused, at analysis **and** again at run time. |

Every detected value (name, mode, silent arguments, extract folder, shortcut, Install-all membership) is editable. Modes: **Silent install**, **Download and run** (interactive), **Download only** (to the Files folder), **Download and extract** (`.zip` built in; `.7z` via Windows 11's `tar.exe`, optional Start Menu shortcut).

**First install = review.** The engine downloads the file, detects the installer technology (Inno → `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART`, NSIS → `/S`, MSI → `/qn /norestart`, WiX Burn → `/quiet /norestart`, Squirrel → `--silent`, InstallShield → `/s /v"/qn"`, MSIX → `Add-AppxPackage`; unknown → *Download and run*), reads the Authenticode signer and the exe's own icon, then stops with a **Review** button. The review sheet shows the publisher and signature; an unsigned file needs an explicit "Install anyway". **Test** runs the install once and shows the exit code (or the error and log on failure). After that the signer is pinned — a later download signed by someone else fails. The download is cached, so reviewing doesn't download twice.

The first install also snapshots the uninstall registry (and MSIX packages) before/after and stores the new key, which gives reliable *Installed* detection and an **Uninstall** action.

Cards behave like built-in ones (progress ring, retry + details, reinstall) and go through the **same engine**: `Engine::item()` returns built-in catalog items first, then My Apps entries converted to catalog `Item`s. Context menu (right-click or `…`): Edit, Change link, Duplicate, Include in "Install all", Move up/down, Uninstall, Remove. Drag cards to reorder.

**Persistence.** `%APPDATA%\SetupHub\my_apps.json` (versioned schema, validated on load; an invalid file is moved aside, never overwritten). Settings → My Apps: **Export / Import** (import shows every link and argument for review before anything is added; imported entries must be reviewed again on the new PC), **My Apps list URL** (raw GitHub, Google Drive file link or any HTTPS URL — loaded on first launch when the list is empty, offered for merge), and **Allow plain HTTP links** (off by default). **Check links** HEADs every URL and marks broken ones with a red dot and *Change link*.

Runtime-only safety flags (`custom`, `reviewed`, `allow_unsigned`, `allow_http`) can't be set by a catalog file — they are skipped when deserializing.

## Decisions (and deviations from the brief)

- **Signature check** uses `Get-AuthenticodeSignature` (a WinVerifyTrust wrapper) instead of raw WinVerifyTrust FFI — same trust decision, also returns the signer name, ~40 lines less unsafe code.
- **GPU detection** and **license status** read WMI through PowerShell `Get-CimInstance … | ConvertTo-Json` instead of the `wmi` crate. License status comes from `SoftwareLicensingProduct` (what `slmgr /dli /xpr` read) so it is locale-independent; the product key itself still goes through `slmgr /ipk` + `/ato`.
- **Registry** via `winreg`, `SystemParametersInfo` via `windows-sys`, Mica via Tauri's built-in `windowEffects` (no `window-vibrancy` crate needed). Windows 10 gets a solid background.
- **Spotify** (refuses elevated installs) runs through a one-shot scheduled task with `/RL LIMITED` as the signed-in user; Setup Hub waits for its exit code and deletes the task.
- **Winget download progress** is parsed from winget's own "x MB / y MB" output.
- **Filled-button blue** is `#0071E3` in both themes: white text on `#007AFF`/`#0A84FF` is only 4.0/3.6 : 1, below the required 4.5 : 1. `#0A84FF`-family blues are still used for links and icons in dark mode. The Windows accent color is not applied for the same reason (arbitrary accents break contrast).
- **GIGABYTE Control Center**: gigabyte.com's lookup API is behind bot protection, so the catalog pins the current `GCC_26.09.10.01.zip` on `download.gigabyte.com`; update it via the remote catalog. Its setup has no documented silent switch, so it opens for you (*Needs your clicks*). ~900 MB.
- **GearUP Booster** ships a custom 7-Zip SFX installer with no documented silent switch — *Needs your clicks*.
- **JONSBO PC Monitor** (Jonsbo TH-360 LCD) is resolved from the TH-360 product page on jonsbo.com, so new versions are picked up automatically; the zip's NSIS setup must be signed by Dongguan Hongtai Technology (JONSBO's company) and runs with `/S`.
- **VLC media player** installs through winget only (`VideoLAN.VLC`): download.videolan.org redirects to ~70 third-party mirrors, which a per-item host allow-list can't cover.
- **Google Chrome** installs through winget (`Google.Chrome`), falling back to Google's enterprise MSI (`googlechromestandaloneenterprise64.msi`, signed by Google).
- **Dev toolchain**: Git (`Git.Git`, fallback: the signed Git for Windows release on GitHub), Node.js LTS, Python 3.14 (winget passes `PrependPath=1`), Python Launcher, uv and FFmpeg come from winget; uv and FFmpeg are winget "portable" packages, linked into `%LOCALAPPDATA%\Microsoft\WinGet\Links` on PATH.
- **ArtCraft Crafting Apps** ([getartcraft.com/apps](https://getartcraft.com/apps)): PhotoCraft, VectorCraft, FilmCraft, LightCraft, EffectCraft, DesignCraft and PdfCraft, each the per-machine `…-windows-x64.msi` from its `storytold/*` GitHub release, checked against GitHub's SHA-256 digest. The ArtCraft studio app itself isn't in the catalog: it's not on that page, and its GitHub releases are drafts.
- **Popular free apps** (Firefox, Brave, Thunderbird, WhatsApp, AnyDesk, LocalSend, Notepad++, Obsidian, Bitwarden, Everything, PowerToys, OBS Studio, HandBrake, Audacity, HWiNFO, CPU-Z, CrystalDiskInfo; Epic Games, EA app, Ubisoft Connect, GOG GALAXY, MSI Afterburner; Blender, Affinity, GIMP, Krita, Inkscape, Paint.NET, Canva, Upscayl; VS Code, Cursor, JetBrains Toolbox, PowerShell 7, GitHub Desktop, GitHub CLI, Docker Desktop, Postman, DBeaver, WinSCP, PuTTY) install through winget only, each ID checked against `microsoft/winget-pkgs`; WhatsApp comes from the Microsoft Store source. Docker Desktop asks for a restart (WSL 2). Left out: Battle.net (its winget package requires an install location and has no silent mode) and RustDesk (not in winget).
- **Flameshot** comes from winget (`Flameshot.Flameshot`), falling back to its GitHub MSI (`Flameshot-<version>-win64.msi`, GitHub's SHA-256). Flameshot registers Print Screen itself (`RegisterHotKey(VK_SNAPSHOT)`), which fails while Windows gives the key to Snipping Tool, and it doesn't start with Windows by default. So after installing it Setup Hub applies the **Print Screen** tweak (`HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled` = 0, previous value kept for Revert), adds `HKCU\…\Run\Flameshot` = `%ProgramFiles%\Flameshot\bin\flameshot.exe` (the same entry Flameshot's own "Launch at startup" writes) and starts it as the signed-in user. Catalog flag: `print_screen: true`, which requires `detect.path` to be an `.exe` under `%ProgramFiles%`.
- **Astrum Play** is a web loader (`astrum-play.ru/loader/AstrumPlayLoader.exe`) with no silent mode — also *Needs your clicks*.
- **AutoLogon** is extracted to `%ProgramFiles%\SetupHub\Tools\Autologon` with a Start Menu shortcut and launched; Setup Hub never types credentials.
- **NVIDIA**: the card comes from WMI (`Win32_VideoController`), or from `HKLM\SYSTEM\CurrentControlSet\Enum\PCI` display devices if PowerShell/WMI fails. A card with PCI vendor `10DE` still on the Basic Display Adapter has no model name yet, so it gets the NVIDIA App. Otherwise product/series IDs come from `lookupValueSearch.aspx` (TypeID 3/4) matched against that name, then `AjaxDriverService DriverManualLookup` with `dch=1, isWHQL=1, upCRD=0` (Game Ready, not Studio). Installed with `-s -noreboot -noeula [-clean]`; a reboot is always offered after.
- **Icons** are fetched once at build time (`pnpm icons`) and bundled, so runtime network calls stay limited to catalog, downloads, NVIDIA lookup and Drive. No telemetry; the only update traffic is the GitHub release check above.

## Ven (Vencord + Discord at sign-in)

`ven/` is a separate small Rust app (~1.5 MB, no .NET, no admin rights). `src-tauri/build.rs` builds it for the same target and Setup Hub embeds the exe. *Install Ven* (Ven page):

1. removes the old **AD** launcher (MyDiscordLauncher: its files, shortcuts and `Run\MyDiscordLauncher` entry);
2. writes `%LOCALAPPDATA%\Programs\Ven\Ven.exe`, adds `HKCU\…\Run\Ven` = `"…\Ven.exe" --startup` and a Start Menu shortcut;
3. runs it once as the signed-in user (not elevated — Discord must not inherit Setup Hub's admin rights) and shows the result.

Every run of `Ven.exe`:

- marks Discord's own Run entry *Disabled* (`Explorer\StartupApproved\Run\Discord`, what Task Manager › Startup apps does — Discord rewrites its Run value but never this flag), so Discord can't open before Vencord is in place;
- waits for the network (up to 2 minutes at sign-in), then updates `%LOCALAPPDATA%\Ven\VencordInstallerCli.exe` from the latest [Vencord/Installer](https://github.com/Vencord/Installer) release when its SHA-256 (GitHub's asset digest, required) differs — only GitHub hosts are followed;
- runs `VencordInstallerCli -install -branch stable`: the installer downloads the latest [Vencord](https://github.com/Vendicated/Vencord) when it changed, closes Discord if it's open, and patches the newest Discord version (Vencord's own `persistAfterDiscordUpdates` keeps it in place across Discord updates in between);
- starts Discord with `Update.exe --processStart Discord.exe` (plus `--process-start-args --start-minimized` at sign-in, like Discord's own autostart). Offline, it skips the update and still starts Discord.

Each run is logged to `%LOCALAPPDATA%\Ven\ven.log`; `status.json` holds the last result, which the Ven page shows with **Run now** and **Remove Ven** (removes Ven, gives Discord its autostart back; Vencord stays in Discord).

Why the old AD didn't work: its sign-in mode only ran after you clicked *Enable* in its window, it exited without starting Discord whenever Discord was already patched, and it needed the .NET 8 Desktop Runtime.

## Layout

```
src/                     React UI (App shell, pages/, components.tsx, api.ts job store, i18n/en.json + ru.json)
src-tauri/src/
  lib.rs                 Tauri commands + setup
  engine.rs              job pipeline, queues, events
  catalog.rs             schema, remote override, GitHub/scrape resolution
  download.rs  sig.rs    resumable downloads, SHA-256 + Authenticode policy
  installer.rs           winget, installers, de-elevation, unzip
  detect.rs              installed-state detection
  defaults.rs            default apps: associations policy XML
  update.rs              self-update from GitHub Releases, app update checks
  ven.rs                 installs/runs/removes Ven (embedded from ../ven)
  tweaks.rs gpu.rs drive.rs activation.rs settings.rs util.rs
  bin/verify_catalog.rs  build-time link verification → catalog-report.md
QA-CHECKLIST.md          manual test plan for a clean Windows 11 VM
```
