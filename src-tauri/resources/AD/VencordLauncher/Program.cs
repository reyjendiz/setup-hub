using System.Diagnostics;

static class Program
{
    internal const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    internal static readonly string DataDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "MyDiscordLauncher");
    internal static readonly string CliPath = Path.Combine(DataDir, "VencordInstallerCli.exe");
    internal static readonly string PathFile = Path.Combine(DataDir, "discord_path.txt");
    static readonly string LogFile = Path.Combine(DataDir, "log.txt");
    internal const string CliUrl = "https://github.com/Vencord/Installer/releases/latest/download/VencordInstallerCli.exe";

    [STAThread]
    static void Main(string[] args)
    {
        if (args.Contains("--silent")) { Silent(); return; }

        Directory.CreateDirectory(DataDir);
        File.SetAttributes(DataDir, File.GetAttributes(DataDir) | FileAttributes.Hidden);
        ApplicationConfiguration.Initialize();
        Application.Run(new Ui());
    }

    // ---------- Mode 2: no UI ----------
    static void Silent()
    {
        try
        {
            var exe = File.ReadAllText(PathFile).Trim();
            var dir = AppDir(exe) ?? throw new Exception("no Discord app dir next to " + exe);
            if (File.Exists(Path.Combine(dir, "resources", "_app.asar"))) return; // patched: do nothing, Discord stays closed

            if (!File.Exists(CliPath)) throw new Exception("Vencord CLI not downloaded");
            using var p = Process.Start(new ProcessStartInfo(CliPath, $"-install -location \"{dir}\"")
            { CreateNoWindow = true, UseShellExecute = false })!;
            p.WaitForExit();
            if (p.ExitCode != 0) throw new Exception("installer exit code " + p.ExitCode);
            Launch(exe); // only after a successful (re)patch
        }
        catch (Exception e)
        {
            Directory.CreateDirectory(DataDir);
            File.WriteAllText(LogFile, e.ToString());
        }
    }

    static void Launch(string exe)
    {
        var args = Path.GetFileName(exe).Equals("Update.exe", StringComparison.OrdinalIgnoreCase) ? "--processStart Discord.exe" : "";
        Process.Start(new ProcessStartInfo(exe, args) { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(exe)! });
    }

    // Folder holding resources\: the exe's own dir, or newest app-* next to Update.exe.
    internal static string? AppDir(string exe)
    {
        var d = Path.GetDirectoryName(exe)!;
        if (Directory.Exists(Path.Combine(d, "resources"))) return d;
        return Directory.GetDirectories(d, "app-*")
            .OrderBy(x => Version.TryParse(Path.GetFileName(x)[4..], out var v) ? v : new Version())
            .LastOrDefault();
    }
}
