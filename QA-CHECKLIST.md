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
- [ ] No winget (VM without App Installer registered): first install registers it; if that fails, vendor fallbacks run and winget-only apps (CapCut, Claude, qBittorrent) fail with "winget is not available…".
- [ ] AutoLogon → extracted to `C:\Program Files\SetupHub\Tools\Autologon`, Start Menu shortcut "Sysinternals AutoLogon", tool opens; nothing is typed into it.
- [ ] Search field and Ctrl+K palette filter apps; Enter on an app installs it; arrows move the selection; Esc closes.

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
- [ ] Hibernation / Fast Startup toggles apply and revert; `powercfg /a` reflects the change.
- [ ] Re-applying a tweak twice and then reverting restores the *original* values (snapshot isn't overwritten).

## Drivers
- [ ] RTX card detected with installed version in NVIDIA format (e.g. 617.42).
- [ ] Up-to-date driver → green "Up to date"; outdated → "Update available" + Install.
- [ ] Install with **Clean install** on → screen flashes; restart banner appears.
- [ ] Block `gfwsl.geforce.com` in hosts → page shows the lookup error; Install falls back to the NVIDIA App.
- [ ] VM with only Basic Display Adapter → "No supported GPU detected"; no install button.

## AD
- [ ] Without Discord → "Install Discord, then AD" installs Discord then AD.
- [ ] With Discord → **Install AD** installs .NET 8 Desktop Runtime if missing, writes `%LOCALAPPDATA%\Programs\AD\MyDiscordLauncher.exe`, Desktop + Start Menu "AD" shortcuts, pre-fills Discord path and downloads Vencord CLI, then opens AD with the first two rows green.

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
