package main

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"syscall"
)

type config struct {
	DiscordExe string `json:"discord_exe"`
}

// Newest app-* dir next to Update.exe, or the exe's own dir if it holds resources.
func appDir(exe string) string {
	d := filepath.Dir(exe)
	if _, err := os.Stat(filepath.Join(d, "resources")); err == nil {
		return d
	}
	m, _ := filepath.Glob(filepath.Join(d, "app-*"))
	if len(m) == 0 {
		return ""
	}
	sort.Strings(m) // ponytail: lexical sort; breaks if build number digit count changes (9999 -> 10000)
	return m[len(m)-1]
}

func main() {
	self, _ := os.Executable()
	home := filepath.Dir(self)

	logf, _ := os.OpenFile(filepath.Join(home, "launcher.log"), os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o644)
	fail := func(msg string) {
		if logf != nil {
			logf.WriteString(msg + "\n")
		}
	}

	var cfg config
	raw, err := os.ReadFile(filepath.Join(home, "config.json"))
	if err == nil {
		err = json.Unmarshal(raw, &cfg)
	}
	if err != nil || cfg.DiscordExe == "" {
		fail("config.json: " + errStr(err))
		return
	}

	if dir := appDir(cfg.DiscordExe); dir == "" {
		fail("no Discord app dir found next to " + cfg.DiscordExe)
	} else if _, err := os.Stat(filepath.Join(dir, "resources", "_app.asar")); err != nil {
		// not patched: run installer synchronously, hidden, before Discord starts
		cmd := exec.Command(filepath.Join(home, "VencordInstallerCLI.exe"), "-install", "-location", dir)
		cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true, CreationFlags: 0x08000000} // CREATE_NO_WINDOW
		if out, err := cmd.CombinedOutput(); err != nil {
			fail("installer: " + err.Error() + "\n" + string(out))
		}
	}

	var args []string
	if strings.EqualFold(filepath.Base(cfg.DiscordExe), "Update.exe") {
		args = []string{"--processStart", "Discord.exe"}
	}
	if err := exec.Command(cfg.DiscordExe, args...).Start(); err != nil {
		fail("launch: " + err.Error())
	}
}

func errStr(err error) string {
	if err == nil {
		return "discord_exe empty"
	}
	return err.Error()
}
