// Build-time: fetch each app's official icon once into public/icons/<id>.png so the app makes no icon
// requests at runtime. Tries the vendor's apple-touch-icon, then GitHub avatars, then Google's favicon cache.
import { mkdirSync, writeFileSync } from "node:fs";

const dash = (name) => `https://raw.githubusercontent.com/homarr-labs/dashboard-icons/main/png/${name}.png`;
const choco = (name) => `https://raw.githubusercontent.com/chocolatey-community/chocolatey-packages/master/icons/${name}.png`;
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
  uv: [dash("astral"), "docs.astral.sh"],
  ffmpeg: [null, "ffmpeg.org"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  firefox: ["https://raw.githubusercontent.com/alrra/browser-logos/main/src/firefox/firefox_128x128.png", "mozilla.org"],
  brave: ["https://raw.githubusercontent.com/alrra/browser-logos/main/src/brave/brave_128x128.png", "brave.com"],
  thunderbird: [dash("thunderbird"), "thunderbird.net"],
  whatsapp: [dash("whatsapp"), "whatsapp.com"],
  anydesk: [dash("anydesk"), "anydesk.com"],
  localsend: ["https://raw.githubusercontent.com/localsend/localsend/main/app/assets/img/logo-512.png", "localsend.org"],
  notepadpp: [null, "notepad-plus-plus.org"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  obsidian: [dash("obsidian"), "obsidian.md"],
  bitwarden: ["https://raw.githubusercontent.com/bitwarden/clients/main/apps/desktop/resources/icon.png", "bitwarden.com"],
  everything: [choco("everything"), "voidtools.com"],
  powertoys: ["https://raw.githubusercontent.com/microsoft/PowerToys/main/doc/images/icons/PowerToys%20icon/PNG/PowerToysAppList.targetsize-256.png", "learn.microsoft.com"],
  obs: ["https://raw.githubusercontent.com/obsproject/obs-studio/master/frontend/forms/images/obs.png", "obsproject.com"],
  handbrake: [dash("handbrake"), "handbrake.fr"],
  audacity: [dash("audacity"), "audacityteam.org"],
  hwinfo: [null, "hwinfo.com"], // committed icon rendered from Papirus apps/hwinfo.svg (generic glyph)
  cpuz: ["https://raw.githubusercontent.com/mkevenaar/chocolatey-packages/master/icons/cpu-z.png", "cpuid.com"],
  crystaldiskinfo: [null, "crystalmark.info"], // committed icon converted from hiyohiyo/CrystalDiskInfo resN/DiskInfo.ico
  epic: [dash("epic-games"), "epicgames.com"],
  eaapp: [null, "ea.com"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  ubisoft: [null, "ubisoft.com"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  gog: [null, "gog.com"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  afterburner: [null, "msi.com"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  blender: [dash("blender"), "blender.org"],
  affinity: [null, "affinity.serif.com"], // committed icon rendered from pheralb/svgl static/library/affinity_designer.svg
  gimp: [dash("gimp"), "gimp.org"],
  krita: [null, "krita.org"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  inkscape: [null, "inkscape.org"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  paintnet: [choco("paint.net"), "getpaint.net"],
  canva: [null, "canva.com"], // committed icon rendered from pheralb/svgl static/library/canva.svg
  upscayl: ["https://raw.githubusercontent.com/upscayl/upscayl/main/build/icon.png", "upscayl.org"],
  vscode: ["https://raw.githubusercontent.com/microsoft/vscode/main/resources/win32/code_150x150.png", "code.visualstudio.com"],
  cursor: [null, "cursor.com"], // committed icon simple-icons glyph on its brand colour, rendered to PNG
  jetbrains: [dash("jetbrains-toolbox"), "jetbrains.com"],
  powershell: [dash("powershell"), "learn.microsoft.com"],
  githubdesktop: ["https://raw.githubusercontent.com/desktop/desktop/development/app/static/linux/icon-logo.png", "desktop.github.com"],
  ghcli: [dash("github"), "cli.github.com"],
  docker: [dash("docker"), "docker.com"],
  postman: [dash("postman"), "postman.com"],
  dbeaver: ["https://raw.githubusercontent.com/dbeaver/dbeaver/devel/plugins/org.jkiss.dbeaver.ui.app.standalone/icons/dbeaver256.png", "dbeaver.io"],
  winscp: [null, "winscp.net"], // committed icon converted from winscp/winscp source/resource/Application.ico
  putty: [dash("putty"), "putty.org"],
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
