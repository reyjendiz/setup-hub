using System.Drawing.Drawing2D;
using System.Drawing.Text;
using System.Runtime.InteropServices;
using Microsoft.Win32;

// Apple-HIG-style setup window: grouped list card, pill buttons, semantic light/dark colors, 44px targets, 8pt grid.
class Ui : Form
{
    record Pal(Color Bg, Color Card, Color Label, Color Secondary, Color Sep, Color Fill, Color Blue, Color Link, Color Green, Color Red);

    static readonly Pal Light = new(
        Color.FromArgb(242, 242, 247), Color.White, Color.Black, Color.FromArgb(108, 108, 112), Color.FromArgb(198, 198, 200),
        Color.FromArgb(239, 239, 244), Color.FromArgb(0, 113, 227), Color.FromArgb(0, 102, 204), Color.FromArgb(36, 138, 61), Color.FromArgb(215, 0, 21));
    static readonly Pal Dark = new(
        Color.Black, Color.FromArgb(28, 28, 30), Color.White, Color.FromArgb(152, 152, 159), Color.FromArgb(56, 56, 58),
        Color.FromArgb(44, 44, 46), Color.FromArgb(10, 132, 255), Color.FromArgb(41, 151, 255), Color.FromArgb(48, 209, 88), Color.FromArgb(255, 69, 58));

    static bool IsDark()
    {
        try { return Registry.GetValue(@"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "AppsUseLightTheme", 1) is 0; }
        catch { return false; }
    }

    [DllImport("dwmapi.dll")] static extern int DwmSetWindowAttribute(IntPtr h, int attr, ref int v, int size);
    static int Ref(Color c) => c.R | c.G << 8 | c.B << 16;

    readonly bool dark = IsDark();
    readonly Pal c;
    readonly float k;
    readonly Font fTitle, fBody, fFoot, fBtn;
    readonly Card card;
    readonly Label msg;
    readonly PillButton[] btn = new PillButton[3];
    readonly string[] titles = { "Discord", "Vencord installer", "Start with Windows" };
    readonly string[] details = new string[3];
    readonly bool[] done = new bool[3];

    int S(int v) => (int)Math.Round(v * k);

    public Ui()
    {
        c = dark ? Dark : Light;
        k = DeviceDpi / 96f;
        var fam = new InstalledFontCollection().Families.Select(f => f.Name)
            .FirstOrDefault(n => n is "Manrope" or "Segoe UI Variable Text" or "Segoe UI") ?? "Segoe UI";
        Font F(float pt, FontStyle s = FontStyle.Regular) => new(fam, pt, s, GraphicsUnit.Point);
        fTitle = F(21, FontStyle.Bold); fBody = F(12); fFoot = F(9.75f); fBtn = F(11.25f, FontStyle.Bold);

        Text = "Discord + Vencord";
        AutoScaleMode = AutoScaleMode.None;
        BackColor = c.Bg;
        FormBorderStyle = FormBorderStyle.FixedSingle;
        MaximizeBox = false;
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(S(480), S(400));

        int m = S(24), w = S(432);
        Controls.Add(new Label { Text = "Discord + Vencord", Font = fTitle, ForeColor = c.Label, BackColor = c.Bg, Bounds = new(m, S(24), w, S(40)) });
        Controls.Add(new Label { Text = "Keeps Vencord patched after Discord updates.", Font = fBody, ForeColor = c.Secondary, BackColor = c.Bg, Bounds = new(m, S(64), w, S(24)) });

        card = new Card(this) { Bounds = new(m, S(104), w, S(72) * 3), BackColor = c.Card };
        Controls.Add(card);

        Action[] acts = { Pick, Download, Startup };
        for (int i = 0; i < 3; i++)
        {
            int idx = i;
            var b = new PillButton(this) { Bounds = new(w - S(16) - S(104), S(72) * i + S(14), S(104), S(44)), BackColor = c.Card };
            b.Click += (_, _) => acts[idx]();
            btn[i] = b;
            card.Controls.Add(b);
        }

        msg = new Label { Font = fFoot, ForeColor = c.Secondary, BackColor = c.Bg, Bounds = new(m + S(4), S(104) + S(72) * 3 + S(12), w - S(8), S(48)) };
        Controls.Add(msg);

        Reload();
    }

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        int d = dark ? 1 : 0, cap = Ref(c.Bg), txt = Ref(c.Label);
        DwmSetWindowAttribute(Handle, 20, ref d, 4);   // dark title bar
        DwmSetWindowAttribute(Handle, 35, ref cap, 4); // caption = page background
        DwmSetWindowAttribute(Handle, 36, ref txt, 4);
    }

    void Say(string text, bool error = false) { msg.ForeColor = error ? c.Red : c.Secondary; msg.Text = text; }

    // Recompute row state from disk/registry.
    void Reload()
    {
        var path = File.Exists(Program.PathFile) ? File.ReadAllText(Program.PathFile).Trim() : "";
        done[0] = path != "";
        details[0] = done[0] ? path : "Not selected";
        done[1] = File.Exists(Program.CliPath);
        details[1] = done[1] ? "Ready" : "Not downloaded";
        using (var k = Registry.CurrentUser.OpenSubKey(Program.RunKey))
            done[2] = k?.GetValue("MyDiscordLauncher") is not null;
        details[2] = done[2] ? "On, runs silently at sign-in" : "Off";

        string[] todo = { "Select", "Download", "Enable" }, redo = { "Change", "Update", "Re-apply" };
        int first = Array.IndexOf(done, false);
        for (int i = 0; i < 3; i++)
        {
            btn[i].Text = done[i] ? redo[i] : todo[i];
            btn[i].Primary = i == first;
            btn[i].Invalidate();
        }
        card.Invalidate();
    }

    void Pick()
    {
        using var f = new OpenFileDialog
        {
            Title = "Select Discord",
            Filter = "Discord (Update.exe;Discord.exe)|Update.exe;Discord.exe",
            InitialDirectory = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Discord"),
        };
        if (f.ShowDialog() != DialogResult.OK) return;
        File.WriteAllText(Program.PathFile, f.FileName);
        Reload();
        if (Program.AppDir(f.FileName) is null) Say("No Discord app folder found next to that file.", true);
        else Say("");
    }

    async void Download()
    {
        foreach (var b in btn) b.Enabled = false;
        Say("Downloading Vencord installer…");
        try
        {
            using var http = new HttpClient();
            var tmp = Program.CliPath + ".tmp";
            await File.WriteAllBytesAsync(tmp, await http.GetByteArrayAsync(Program.CliUrl));
            File.Move(tmp, Program.CliPath, true);
            Say("");
        }
        catch (Exception e) { Say("Download failed: " + e.Message, true); }
        foreach (var b in btn) b.Enabled = true;
        Reload();
    }

    void Startup()
    {
        using (var k = Registry.CurrentUser.CreateSubKey(Program.RunKey))
        {
            k.SetValue("MyDiscordLauncher", $"\"{Environment.ProcessPath}\" --silent");
            k.DeleteValue("Discord", false); // Discord's own autostart
        }
        Reload();
        Say("Keep this app where it is. Its location is stored for sign-in.");
    }

    // Grouped-list card: rounded surface, hairline separators, status dot, title + detail per row.
    class Card : Panel
    {
        readonly Ui u;
        public Card(Ui ui) { u = ui; DoubleBuffered = true; }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(u.c.Bg);
            using (var p = Round(new Rectangle(0, 0, Width - 1, Height - 1), u.S(16)))
            using (var br = new SolidBrush(u.c.Card)) g.FillPath(br, p);

            int rh = u.S(72);
            const TextFormatFlags fl = TextFormatFlags.Left | TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis | TextFormatFlags.NoPadding;
            for (int i = 0; i < 3; i++)
            {
                int y = rh * i, x = u.S(20), d = u.S(10);
                if (i > 0) using (var pen = new Pen(u.c.Sep)) g.DrawLine(pen, x, y, Width - u.S(16), y);

                var dot = new Rectangle(x, y + (rh - d) / 2, d, d);
                if (u.done[i]) using (var br = new SolidBrush(u.c.Green)) g.FillEllipse(br, dot);
                else using (var pen = new Pen(u.c.Secondary, Math.Max(1.5f, u.k * 1.5f))) g.DrawEllipse(pen, dot);

                int tx = x + d + u.S(14), tw = btn0Left() - tx - u.S(12);
                TextRenderer.DrawText(g, u.titles[i], u.fBody, new Rectangle(tx, y + u.S(12), tw, u.S(24)), u.c.Label, fl);
                TextRenderer.DrawText(g, u.details[i], u.fFoot, new Rectangle(tx, y + u.S(36), tw, u.S(22)), u.c.Secondary, fl | TextFormatFlags.PathEllipsis);
            }
        }

        int btn0Left() => Width - u.S(16) - u.S(104);
    }

    // Pill button: filled accent for the next step, gray fill otherwise. Hover/press states, focus ring.
    class PillButton : Button
    {
        readonly Ui u;
        bool hot, down;
        public bool Primary;

        public PillButton(Ui ui)
        {
            u = ui;
            SetStyle(ControlStyles.UserPaint | ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer, true);
            FlatStyle = FlatStyle.Flat;
            FlatAppearance.BorderSize = 0;
            Cursor = Cursors.Hand;
            Font = ui.fBtn;
        }

        protected override void OnMouseEnter(EventArgs e) { hot = true; Invalidate(); base.OnMouseEnter(e); }
        protected override void OnMouseLeave(EventArgs e) { hot = down = false; Invalidate(); base.OnMouseLeave(e); }
        protected override void OnMouseDown(MouseEventArgs e) { down = true; Invalidate(); base.OnMouseDown(e); }
        protected override void OnMouseUp(MouseEventArgs e) { down = false; Invalidate(); base.OnMouseUp(e); }
        protected override void OnGotFocus(EventArgs e) { Invalidate(); base.OnGotFocus(e); }
        protected override void OnLostFocus(EventArgs e) { Invalidate(); base.OnLostFocus(e); }
        protected override void OnEnabledChanged(EventArgs e) { Invalidate(); base.OnEnabledChanged(e); }

        protected override void OnPaint(PaintEventArgs e)
        {
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(u.c.Card);

            var fill = Primary ? u.c.Blue : u.c.Fill;
            var fg = Primary ? Color.White : u.c.Link;
            if (down) fill = Shade(fill, u.dark ? 0.75f : 0.85f);
            else if (hot) fill = Shade(fill, u.dark ? 1.15f : 0.93f);
            if (!Enabled) { fill = Blend(fill, u.c.Card, 0.5f); fg = Blend(fg, u.c.Card, 0.5f); }

            var r = new Rectangle(1, 1, Width - 3, Height - 3);
            using (var p = Round(r, r.Height))
            using (var br = new SolidBrush(fill)) g.FillPath(br, p);

            if (Focused && ShowFocusCues)
            {
                var fr = new Rectangle(0, 0, Width - 1, Height - 1);
                using var p = Round(fr, fr.Height);
                using var pen = new Pen(u.c.Link, 2);
                g.DrawPath(pen, p);
            }
            TextRenderer.DrawText(g, Text, Font, ClientRectangle, fg, TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPadding);
        }

        static Color Shade(Color c, float f) => Color.FromArgb(c.A, (int)Math.Min(255, c.R * f), (int)Math.Min(255, c.G * f), (int)Math.Min(255, c.B * f));
        static Color Blend(Color a, Color b, float t) => Color.FromArgb((int)(a.R * t + b.R * (1 - t)), (int)(a.G * t + b.G * (1 - t)), (int)(a.B * t + b.B * (1 - t)));
    }

    static GraphicsPath Round(Rectangle r, int d)
    {
        d = Math.Min(d, Math.Min(r.Width, r.Height));
        var p = new GraphicsPath();
        p.AddArc(r.X, r.Y, d, d, 180, 90);
        p.AddArc(r.Right - d, r.Y, d, d, 270, 90);
        p.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
        p.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
        p.CloseFigure();
        return p;
    }
}
