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
  claude: ["https://claude.ai/apple-touch-icon.png", "claude.ai"],
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
  ad: [null, "vencord.dev"],
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
