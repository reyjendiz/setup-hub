// Build-time: fetch each app's official icon once into public/icons/<id>.png so the app makes no icon
// requests at runtime. Tries the vendor's apple-touch-icon, then GitHub avatars, then Google's favicon cache.
import { mkdirSync, writeFileSync } from "node:fs";

const craft = (id) => `https://raw.githubusercontent.com/storytold/${id}/HEAD/assets/app-icon/hicolor/128x128/apps/ai.storyteller.${id}.png`;

const SOURCES = {
  zoom: ["https://zoom.us/apple-touch-icon.png", "zoom.us"],
  steam: ["https://store.steampowered.com/favicon.ico", "store.steampowered.com"],
  telegram: ["https://telegram.org/img/apple-touch-icon.png", "telegram.org"],
  capcut: [null, "capcut.com"],
  gearup: [null, "gearupbooster.com"],
  nordvpn: [null, "nordvpn.com"],
  discord: [null, "discord.com"],
  figma: ["https://static.figma.com/app/icon/1/touch-180.png", "figma.com"],
  spotify: [null, "spotify.com"],
  gcc: [null, "gigabyte.com"],
  astrum: ["https://astrum-play.ru/hotbox/favicon/favicon-96x96.png", "astrum-play.ru"],
  autologon: [null, "learn.microsoft.com"],
  ava: ["https://avamodmanager.com/assets/img/apple-touch-icon.png", "avamodmanager.com"],
  compresso: ["https://github.com/codeforreal1.png?size=128", "compresso.codeforreal.com"],
  autopara: ["https://github.com/sevcenkoa864-oss.png?size=128", "github.com"],
  pdfcraft: [craft("pdfcraft"), "getartcraft.com"],
  photocraft: [craft("photocraft"), "getartcraft.com"],
  vectorcraft: [craft("vectorcraft"), "getartcraft.com"],
  filmcraft: [craft("filmcraft"), "getartcraft.com"],
  lightcraft: [craft("lightcraft"), "getartcraft.com"],
  effectcraft: [craft("effectcraft"), "getartcraft.com"],
  designcraft: [craft("designcraft"), "getartcraft.com"],
  qbittorrent: [null, "qbittorrent.org"],
  ven: [null, "vencord.dev"],
  nanazip: ["https://raw.githubusercontent.com/M2Team/NanaZip/main/Assets/NanaZip.png", "github.com"],
  onlyoffice: [null, "onlyoffice.com"],
  viber: [null, "viber.com"],
  jonsbo: [null, "jonsbo.com"],
  vlc: ["https://raw.githubusercontent.com/videolan/vlc/3.0.x/share/icons/128x128/vlc.png", "videolan.org"],
  chrome: ["https://raw.githubusercontent.com/alrra/browser-logos/main/src/chrome/chrome_128x128.png", "google.com"],
  python: ["https://raw.githubusercontent.com/python/cpython/main/PC/icons/logox128.png", "python.org"],
  pylauncher: ["https://raw.githubusercontent.com/python/cpython/main/PC/icons/py.png", "python.org"],
  nodejs: ["https://raw.githubusercontent.com/nodejs/nodejs.org/main/apps/site/public/static/images/favicons/android-chrome-192x192.png", "nodejs.org"],
  git: [null, "git-scm.com"], // committed icon converted from git-for-windows/build-extra's git.ico
  uv: [null, "docs.astral.sh"],
  ffmpeg: [null, "ffmpeg.org"],
  firefox: ["https://raw.githubusercontent.com/alrra/browser-logos/main/src/firefox/firefox_128x128.png", "mozilla.org"],
  brave: ["https://raw.githubusercontent.com/alrra/browser-logos/main/src/brave/brave_128x128.png", "brave.com"],
  thunderbird: [null, "thunderbird.net"],
  whatsapp: [null, "whatsapp.com"],
  anydesk: [null, "anydesk.com"],
  localsend: ["https://raw.githubusercontent.com/localsend/localsend/main/app/assets/img/logo-512.png", "localsend.org"],
  notepadpp: [null, "notepad-plus-plus.org"],
  obsidian: [null, "obsidian.md"],
  bitwarden: ["https://raw.githubusercontent.com/bitwarden/clients/main/apps/desktop/resources/icon.png", "bitwarden.com"],
  everything: [null, "voidtools.com"],
  powertoys: [null, "learn.microsoft.com"],
  obs: ["https://raw.githubusercontent.com/obsproject/obs-studio/master/frontend/forms/images/obs.png", "obsproject.com"],
  handbrake: [null, "handbrake.fr"],
  audacity: [null, "audacityteam.org"],
  hwinfo: [null, "hwinfo.com"],
  cpuz: [null, "cpuid.com"],
  crystaldiskinfo: [null, "crystalmark.info"],
  epic: [null, "epicgames.com"],
  eaapp: [null, "ea.com"],
  ubisoft: [null, "ubisoft.com"],
  gog: [null, "gog.com"],
  afterburner: [null, "msi.com"],
  blender: [null, "blender.org"],
  affinity: [null, "affinity.serif.com"],
  gimp: [null, "gimp.org"],
  krita: [null, "krita.org"],
  inkscape: [null, "inkscape.org"],
  paintnet: [null, "getpaint.net"],
  canva: [null, "canva.com"],
  upscayl: ["https://raw.githubusercontent.com/upscayl/upscayl/main/build/icon.png", "upscayl.org"],
  vscode: ["https://raw.githubusercontent.com/microsoft/vscode/main/resources/win32/code_150x150.png", "code.visualstudio.com"],
  cursor: [null, "cursor.com"],
  jetbrains: [null, "jetbrains.com"],
  powershell: [null, "learn.microsoft.com"],
  githubdesktop: [null, "desktop.github.com"],
  ghcli: [null, "cli.github.com"],
  docker: [null, "docker.com"],
  postman: [null, "postman.com"],
  dbeaver: ["https://raw.githubusercontent.com/dbeaver/dbeaver/devel/plugins/org.jkiss.dbeaver.ui.app.standalone/icons/dbeaver256.png", "dbeaver.io"],
  winscp: [null, "winscp.net"],
  putty: [null, "putty.org"],
  flameshot: ["https://raw.githubusercontent.com/flameshot-org/flameshot/HEAD/data/img/hicolor/128x128/apps/flameshot.png", "flameshot.org"],
};
const only = process.argv.slice(2);

const isImage = (r, b) => r.ok && /^image\//.test(r.headers.get("content-type") ?? "") && b.byteLength > 600;

mkdirSync("public/icons", { recursive: true });
for (const [id, [direct, domain]] of Object.entries(SOURCES)) {
  if (only.length && !only.includes(id)) continue;
  const tries = [direct, `https://${domain}/apple-touch-icon.png`, `https://www.google.com/s2/favicons?domain=${domain}&sz=128`].filter(Boolean);
  let ok = false;
  for (const url of tries) {
    try {
      const r = await fetch(url, { headers: { "User-Agent": "Mozilla/5.0" }, redirect: "follow" });
      const b = await r.arrayBuffer();
      if (isImage(r, b) && !(r.headers.get("content-type") ?? "").includes("icon")) {
        writeFileSync(`public/icons/${id}.png`, Buffer.from(b));
        console.log(`${id.padEnd(12)} ${b.byteLength}B  ${url}`);
        ok = true;
        break;
      }
    } catch {}
  }
  if (!ok) console.log(`${id.padEnd(12)} -- none (monogram fallback)`);
}
