# Manual QA — clean Windows 11 VM

Take a VM snapshot first, then revert between the "fresh" runs. Copy only `setup-hub.exe` into the VM.

## Launch
- [ ] Double-click `setup-hub.exe` → UAC prompt appears (requireAdministrator). Accept → window opens in < 1.5 s with Mica backdrop.
- [ ] Decline UAC → nothing runs (expected; Windows blocks it).
- [ ] Launch a second copy → the first window is focused, no second window.
- [ ] Windows 10 VM: window opens with a solid background (no transparent holes).
- [ ] Idle RAM in Task Manager (setup-hub + its WebView2 processes) < 150 MB.
- [ ] Title bar: drag moves the window; double-click maximizes; min / max / close buttons work; close hover is red.

## Apps
- [ ] On first launch, apps already installed show a green **Installed** pill + version.
- [ ] **Install** one winget app (Steam) → ring with % → spinner "Installing" → ✓ Installed. Log sheet shows each phase.
- [ ] Hover a running button → it reads **Cancel**; click → state "cancelled", button returns to Install.
- [ ] Click ✓ Installed → it turns into **Reinstall**; click outside → reverts; click Reinstall → reinstalls.
- [ ] Select 3 cards with the checkboxes → "Install selected (3)" → all three run; downloads overlap, installs run one at a time.
- [ ] **Install all apps** → global progress bar fills; GCC and Astrum run last and show "The installer window is open".
- [ ] Spotify installs while the app is elevated (de-elevated via scheduled task) and appears under the signed-in user.
- [ ] NordVPN (needs reboot) → restart banner appears at the end; **Later** hides it, **Restart now** restarts in 5 s.
- [ ] Force a failure (disconnect the network mid-download) → red **Retry** + **Details** with the error; reconnect → Retry resumes the partial download.
- [ ] Offline launch → app opens; catalog origin says "embedded"; installs fail with a clear network error, no crash.
- [ ] No winget (VM without App Installer registered): first install registers it; if that fails, vendor fallbacks run and winget-only apps (CapCut, qBittorrent, VLC, Node.js, Python, Python Launcher, uv, FFmpeg) fail with "winget is not available…".
- [ ] AutoLogon → extracted to `C:\Program Files\SetupHub\Tools\Autologon`, Start Menu shortcut "Sysinternals AutoLogon", tool opens; nothing is typed into it.
- [ ] Tabs: **Programs** (default) and **Games** (Steam, GearUP, Astrum Play, AVA Mod Manager) with counts; the chosen tab survives a restart; search filters inside the tab and the counts show matches per tab.
- [ ] Programs tab shows four sections — Everyday, For designers, For programmers, Dev toolchain — each with **Install these (n)** that installs only that section's missing apps; search hides empty sections.
- [ ] Spot-check the new winget apps on a clean VM: Firefox, WhatsApp (Store), PowerToys, OBS Studio, Epic Games Launcher, Blender, Affinity (MSIX → ✓ Installed via the package), VS Code, PowerShell 7, Docker Desktop (restart banner) — each ends ✓ Installed with a version.
- [ ] Programs → **Dev toolchain** group → **Install these (6)** → Git, Node.js, Python, Python Launcher, uv, FFmpeg install. In a *new* terminal: `git --version`, `node -v`, `npm -v`, `python --version`, `py --version`, `uv --version`, `ffmpeg -version` all work, and `bash --version` finds Git Bash.
- [ ] Crafting Apps (PhotoCraft, VectorCraft, FilmCraft, LightCraft, EffectCraft, DesignCraft, PdfCraft) install silently from their GitHub MSIs and show ✓ Installed with a version.
- [ ] Search field and Ctrl+K palette filter apps; Enter on an app installs it; arrows move the selection; Esc closes.

## Updates
- [ ] Publish a release `v9.9.9` with `setup-hub.exe` in a test fork, point `update::REPO` at it, launch → banner "Setup Hub 9.9.9 is available" → **Update and restart** → progress, the window closes and the new version opens (Settings → About shows it), no second UAC prompt; `setup-hub.old.exe` is gone after that start.
- [ ] Release asset with a wrong digest / unsigned file without digest → the banner shows the error, the running exe is untouched.
- [ ] Exe in a read-only folder → clear "can't replace … (is the folder writable?)" error.
- [ ] Install an older Chrome/Node.js with winget (`winget install Google.Chrome --version …`), launch → the app's card shows `old → new` + **Update**, header shows **Update all (n)** → updates; the badge clears when done.
- [ ] A GitHub-sourced app with an older MSI installed (e.g. PhotoCraft) → shows an update; Update installs the new MSI over it.
- [ ] Settings → Updates: toggle off → no check on the next launch; **Check now** shows a spinner, then "Setup Hub x is up to date. Apps with updates: n".
- [ ] Offline launch → no banner, no error popups.

## My Apps
- [ ] **+ Add** in the title bar and **My Apps** in the sidebar both open the Add sheet; nothing is read from the clipboard until **Paste** is pressed.
- [ ] GitHub repo (`https://github.com/M2Team/NanaZip`) → preview: name, publisher, size, type, version; Add → card appears.
- [ ] Direct link (`https://download.cdn.viber.com/desktop/windows/ViberSetup.msi`) → MSI, args `/qn /norestart`.
- [ ] winget ID (`M2Team.NanaZip`) → publisher from winget; free text (`obs studio`) → list of matches, **Use** turns it into a preview.
- [ ] Store ID (`9NKSQGP7F2NH`) → WhatsApp via msstore; installs.
- [ ] Web page (`https://avamodmanager.com/`) → candidate links; page without installers → "Open page in browser".
- [ ] Google Drive file and folder links → "Download to my files folder" → progress on the Files page.
- [ ] Several lines at once → one result per line; **Add all (n)**.
- [ ] Garbage text / `ftp://` → clear error with a hint. Blocked script (`https://x/a.ps1`) → refused. HTTP link → blocked with the Settings hint; after enabling "Allow plain HTTP links" it is accepted.
- [ ] First **Install** of a direct/GitHub entry → downloads, then **Review**: signer shown; unsigned file needs the checkbox; detected type + suggested args prefilled and editable.
- [ ] **Test** in review → "Finished — exit code 0"; a failing installer → error + log lines.
- [ ] After install: card shows ✓ Installed with version (detected via the recorded uninstall key); context menu **Uninstall** removes it.
- [ ] Modes: Download only → file in the Files folder + Show in Explorer; Download and extract (.zip and .7z) → folder + Start Menu shortcut; Download and run → installer window opens.
- [ ] Edit (rename, change mode/args), Change link (re-review required), Duplicate, Remove (program stays installed).
- [ ] Drag to reorder, and Move up/down from the menu with the keyboard; order survives restart.
- [ ] Include in "Install all" off → Apps → Install all skips it; on → included (interactive entries run last).
- [ ] **Check links** → broken link gets a red dot + Change link; GitHub entry with a newer release shows **Update …**.
- [ ] Settings → Export → `my_apps.json`; Import it on a clean VM → review list shows every link/argument; entries need review again.
- [ ] Settings → My Apps list URL (Drive file link of the export) → on a fresh install with an empty list the app offers to merge on launch; **Load now** works anytime.
- [ ] Offline: analysis shows a network error per line; installed detection still works; nothing crashes.
- [ ] Corrupt `my_apps.json` → app starts with an empty list and the bad file is kept as `my_apps.invalid-*.json`.

## Files
- [ ] List loads (skeleton first) with names, sizes and type icons.
- [ ] Download one file → ring → "Downloaded" + **Show in Explorer** opens the folder with the file selected.
- [ ] **Download all** → every file lands in `Downloads\SetupHub-Drive` (sub-folders recreated). Nothing is executed or extracted.
- [ ] Change folder with **Change…** → next download goes there.
- [ ] Add a Google API key in Settings → list reloads via the API with exact sizes.

## Tweaks
- [ ] Pointer speed → Apply → Control Panel › Mouse shows the 5th notch; `MouseSensitivity` = 8. Revert → previous value.
- [ ] Enhance pointer precision → Apply → checkbox off in Control Panel; Revert → back on.
- [ ] Power → Apply → `powercfg /getactivescheme` = High performance; display/sleep/hibernate "Never". Revert → previous plan & timeouts.
- [ ] Print Screen → Apply → `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled` = 0 and Settings › Accessibility › Keyboard shows "Use the Print screen key to open screen capture" off; Revert → previous value (deleted again if it was absent).
- [ ] Install **Flameshot** → the Print Screen tweak shows Applied, `HKCU\…\Run\Flameshot` points at `C:\Program Files\Flameshot\bin\flameshot.exe`, Flameshot is in the tray (not elevated); **PrtScn** opens Flameshot's capture (sign out and in if Snipping Tool still opens); after a restart Flameshot is running again and PrtScn still works.
- [ ] Hibernation / Fast Startup toggles apply and revert; `powercfg /a` reflects the change.
- [ ] Re-applying a tweak twice and then reverting restores the *original* values (snapshot isn't overwritten).

## Default apps
- [ ] Install **VLC** → card says "Set as the default app — Windows applies it at your next sign-in". `HKLM\SOFTWARE\Policies\Microsoft\Windows\System\DefaultAssociationsConfiguration` = `C:\ProgramData\SetupHub\DefaultAssociations.xml`; the XML lists `VLC.mp4` etc. with `Suggested="true"` and no `.iso`/`.zip`/`.rar`.
- [ ] Install **Chrome** and **PdfCraft** → the same XML now also has `http`/`https`/`.html` → `ChromeHTML` and `.pdf` → `PdfCraft.Document`, with a higher `Version`.
- [ ] Tweaks → Default apps shows Chrome, VLC and PdfCraft with "Applies at your next sign-in". Sign out and in → a link opens Chrome, an .mp4/.mkv/.mp3 opens VLC, a PDF opens PdfCraft; the cards show ✓ Applied with "n of n types".
- [ ] Windows 11: pick Media Player for .mp4 in Settings, sign out and in → it stays Media Player (Suggested = applied once). **Apply** again → VLC again after the next sign-in.
- [ ] **Revert** VLC → VLC's rows are gone from the XML (policy and file removed when nothing is left); .mp4 still opens VLC until changed.
- [ ] **Open Settings** opens Settings › Default apps › the app's page.
- [ ] Set `DefaultAssociationsConfiguration` to another path first → installing VLC leaves it untouched and the card offers Open Settings.
- [ ] Windows 11 Home VM: cards show the Home warning; note here whether Windows applied the policy after sign-in.

## Drivers
- [ ] RTX card detected with installed version in NVIDIA format (e.g. 617.42).
- [ ] Up-to-date driver → green "Up to date"; outdated → "Update available" + Install.
- [ ] Install with **Clean install** on → screen flashes; restart banner appears.
- [ ] Block `gfwsl.geforce.com` in hosts → page shows the lookup error and the button reads **Install NVIDIA App**; it installs the NVIDIA App.
- [ ] Fresh Windows with an NVIDIA card and no driver yet (Device Manager: Microsoft Basic Display Adapter) → card "NVIDIA graphics card", driver "none yet", button **Install NVIDIA App** → NVIDIA App installs, opens, detects the card and offers the driver.
- [ ] Rename `powershell.exe`'s path out of reach (or run with WMI service stopped) → the card still appears (read from the registry).
- [ ] VM with only Basic Display Adapter and a non-NVIDIA/virtual GPU → "No supported GPU detected"; no install button.

## Ven
- [ ] PC that had the old AD: **Install Ven** removes `%LOCALAPPDATA%\Programs\AD`, `%LOCALAPPDATA%\MyDiscordLauncher`, the AD shortcuts and `HKCU\…\Run\MyDiscordLauncher`.
- [ ] Without Discord → "Install Discord, then Ven" installs Discord, then Ven.
- [ ] With Discord → **Install Ven** → `%LOCALAPPDATA%\Programs\Ven\Ven.exe`, `HKCU\…\Run\Ven` = `"…\Ven.exe" --startup`, Start Menu "Ven"; Discord opens **with Vencord** (Settings shows the Vencord section) and is **not** elevated (Task Manager › Details › Elevated = No); the page shows all three rows green and "Last run …: Vencord is up to date; Discord started".
- [ ] Task Manager › Startup apps: Discord = Disabled, Ven = Enabled.
- [ ] Restart the PC → after sign-in Discord opens minimized to the tray with Vencord; `%LOCALAPPDATA%\Ven\ven.log` shows the run (installer update skipped or done, `installer: … Successfully patched`).
- [ ] Let Discord update itself (or install an older Discord), restart → Vencord is still there.
- [ ] Unplug the network, restart → Discord still opens; the Ven page shows "offline — Vencord not checked" in orange.
- [ ] **Run now** → Discord closes and reopens with Vencord; the last-run line updates.
- [ ] **Remove Ven** → Run entry, shortcut and folders gone; Discord's startup entry is Enabled again; Vencord still in Discord.

## Activation
- [ ] Status shows edition, "Activated"/grace state and last 5 key characters.
- [ ] **Open Activation settings** opens Settings › Activation.
- [ ] Invalid key format → button disabled. Valid own key → slmgr output shown; key is not in the log file.

## Settings
- [ ] EN/RU switch changes every label instantly and persists after restart.
- [ ] Light / Dark / System theme; System follows Windows live.
- [ ] Parallel downloads 1 → only one download at a time.
- [ ] GitHub token / Google key saved → visible in Credential Manager under "SetupHub"; Clear removes it; never appears in `settings.json` or logs.
- [ ] Custom catalog URL (raw GitHub) → Refresh → "Loaded N apps from remote"; bad URL → falls back to cached/embedded.
- [ ] Export log → file on Desktop, Explorer opens it.

## Accessibility
- [ ] Tab through sidebar, buttons, checkboxes, toggles — visible focus ring everywhere.
- [ ] Windows "Show animations" off → no motion.
