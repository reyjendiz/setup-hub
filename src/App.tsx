import { getCurrentWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "framer-motion";
import {
  AppWindow, Cpu, FolderDown, KeyRound, Minus, Search, Settings as Gear, SlidersHorizontal, Square, Sparkles, X, Copy,
} from "lucide-react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { api, Boot, Item, Settings } from "./api";
import { Button, spring } from "./components";
import { Key, LangCtx, makeT, useT } from "./i18n";
import Apps from "./pages/Apps";
import Files from "./pages/Files";
import Tweaks from "./pages/Tweaks";
import Drivers from "./pages/Drivers";
import AD from "./pages/AD";
import Activation from "./pages/Activation";
import SettingsPage from "./pages/Settings";
import { listen } from "@tauri-apps/api/event";

export type Page = "apps" | "files" | "tweaks" | "drivers" | "ad" | "activation" | "settings";
const NAV: { id: Page; icon: typeof AppWindow }[] = [
  { id: "apps", icon: AppWindow },
  { id: "files", icon: FolderDown },
  { id: "tweaks", icon: SlidersHorizontal },
  { id: "drivers", icon: Cpu },
  { id: "ad", icon: Sparkles },
  { id: "activation", icon: KeyRound },
  { id: "settings", icon: Gear },
];

interface AppCtxT {
  boot: Boot;
  items: Item[];
  setItems: (i: Item[]) => void;
  settings: Settings;
  updateSettings: (p: Partial<Settings>) => void;
  installed: Record<string, string>;
  refreshInstalled: () => void;
  go: (p: Page) => void;
  search: string;
  setSearch: (s: string) => void;
}
const AppCtx = createContext<AppCtxT>(null!);
export const useApp = () => useContext(AppCtx);

function applyTheme(theme: Settings["theme"]) {
  const el = document.documentElement;
  if (theme === "system") el.removeAttribute("data-theme");
  else el.setAttribute("data-theme", theme);
}

export default function App() {
  const [boot, setBoot] = useState<Boot | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.bootstrap().then(setBoot, (e) => setError(String(e)));
  }, []);
  if (error) return <div className="p-8 text-[var(--red)]">{error}</div>;
  if (!boot) return <div className="h-full" />;
  return <Shell boot={boot} />;
}

function Shell({ boot }: { boot: Boot }) {
  const [settings, setSettings] = useState(boot.settings);
  const [items, setItems] = useState(boot.items);
  const [installed, setInstalled] = useState<Record<string, string>>({});
  const [page, setPage] = useState<Page>("apps");
  const [search, setSearch] = useState("");
  const [palette, setPalette] = useState(false);
  const [reboot, setReboot] = useState<string[]>([]);
  const [rebootDismissed, setRebootDismissed] = useState(false);
  const t = useMemo(() => makeT(settings.lang), [settings.lang]);

  useEffect(() => applyTheme(settings.theme), [settings.theme]);
  useEffect(() => {
    document.documentElement.lang = settings.lang;
  }, [settings.lang]);
  useEffect(() => {
    if (boot.os_build < 22000) document.documentElement.classList.add("no-mica");
  }, [boot.os_build]);

  const refreshInstalled = useCallback(() => {
    api.detectInstalled().then(setInstalled, () => {});
  }, []);
  useEffect(refreshInstalled, [refreshInstalled]);

  useEffect(() => {
    const un = listen<{ phase: string; reboot: boolean }>("job", ({ payload }) => {
      if (payload.phase === "done") {
        refreshInstalled();
        if (payload.reboot) api.rebootPending().then((r) => (setReboot(r), setRebootDismissed(false)));
      }
    });
    return () => void un.then((f) => f());
  }, [refreshInstalled]);

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((v) => !v);
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const updateSettings = useCallback((p: Partial<Settings>) => {
    setSettings((s) => {
      const n = { ...s, ...p };
      api.saveSettings(n).catch(() => {});
      return n;
    });
  }, []);

  const ctx: AppCtxT = { boot, items, setItems, settings, updateSettings, installed, refreshInstalled, go: setPage, search, setSearch };

  return (
    <LangCtx.Provider value={{ lang: settings.lang, t }}>
      <AppCtx.Provider value={ctx}>
        <div className="flex h-full flex-col">
          <TitleBar onSearch={() => setPalette(true)} />
          <div className="flex min-h-0 flex-1">
            <Sidebar page={page} setPage={setPage} />
            <main className="relative min-w-0 flex-1 overflow-hidden rounded-tl-xl border-l border-t border-[var(--card-border)] bg-[var(--content)]">
              <AnimatePresence mode="wait">
                <motion.div
                  key={page}
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -4 }}
                  transition={{ duration: 0.2, ease: [0.25, 0.1, 0.25, 1] }}
                  className="scroll h-full px-8 pb-10 pt-6"
                >
                  {page === "apps" && <Apps />}
                  {page === "files" && <Files />}
                  {page === "tweaks" && <Tweaks />}
                  {page === "drivers" && <Drivers />}
                  {page === "ad" && <AD />}
                  {page === "activation" && <Activation />}
                  {page === "settings" && <SettingsPage />}
                </motion.div>
              </AnimatePresence>
              <AnimatePresence>
                {reboot.length > 0 && !rebootDismissed && (
                  <motion.div
                    initial={{ y: 80, opacity: 0 }}
                    animate={{ y: 0, opacity: 1 }}
                    exit={{ y: 80, opacity: 0 }}
                    transition={spring}
                    role="alertdialog"
                    aria-label={t("reboot.title")}
                    className="absolute bottom-[76px] left-1/2 z-40 flex w-[min(560px,90%)] -translate-x-1/2 items-center gap-4 rounded-2xl border border-[var(--card-border)] bg-[var(--sheet)] p-4 backdrop-blur-xl"
                    style={{ boxShadow: "var(--shadow-hover)" }}
                  >
                    <div className="min-w-0 flex-1">
                      <div className="font-semibold">{t("reboot.title")}</div>
                      <div className="text-[13px] text-[var(--secondary)]">{t("reboot.body", { names: reboot.join(", ") })}</div>
                    </div>
                    <Button variant="plain" onClick={() => setRebootDismissed(true)}>{t("reboot.later")}</Button>
                    <Button variant="primary" onClick={() => api.restartNow()}>{t("reboot.now")}</Button>
                  </motion.div>
                )}
              </AnimatePresence>
            </main>
          </div>
          <AnimatePresence>{palette && <CommandPalette close={() => setPalette(false)} />}</AnimatePresence>
        </div>
      </AppCtx.Provider>
    </LangCtx.Provider>
  );
}

function TitleBar({ onSearch }: { onSearch: () => void }) {
  const { t } = useT();
  const w = getCurrentWindow();
  const [max, setMax] = useState(false);
  useEffect(() => {
    w.isMaximized().then(setMax);
    const un = w.onResized(() => w.isMaximized().then(setMax));
    return () => void un.then((f) => f());
  }, [w]);
  const cap = "grid h-full w-[46px] place-items-center text-[var(--label)] transition-colors duration-100 hover:bg-[var(--fill)]";
  return (
    <div data-tauri-drag-region className="flex h-11 shrink-0 items-center pl-4">
      <div data-tauri-drag-region className="flex w-[204px] items-center gap-2 text-[13px] font-semibold">
        <img src="/app-icon.svg" alt="" className="h-[18px] w-[18px]" draggable={false} />
        Setup Hub
      </div>
      <div data-tauri-drag-region className="flex flex-1 justify-center">
        <button
          onClick={onSearch}
          className="flex h-8 w-[min(420px,60%)] cursor-pointer items-center gap-2 rounded-lg bg-[var(--fill)] px-3 text-[13px] text-[var(--secondary)] hover:bg-[var(--fill-hover)]"
        >
          <Search size={14} />
          <span className="flex-1 text-left">{t("common.search")}</span>
          <kbd className="rounded bg-[var(--fill)] px-1.5 text-[11px] font-medium">Ctrl K</kbd>
        </button>
      </div>
      <div className="flex h-full items-start">
        <button className={cap} onClick={() => w.minimize()} aria-label={t("titlebar.minimize")}>
          <Minus size={16} strokeWidth={1.5} />
        </button>
        <button className={cap} onClick={() => w.toggleMaximize()} aria-label={t("titlebar.maximize")}>
          {max ? <Copy size={13} strokeWidth={1.5} className="-scale-x-100" /> : <Square size={13} strokeWidth={1.5} />}
        </button>
        <button className={`${cap} hover:!bg-[#c42b1c] hover:!text-white`} onClick={() => w.close()} aria-label={t("titlebar.close")}>
          <X size={17} strokeWidth={1.5} />
        </button>
      </div>
    </div>
  );
}

function Sidebar({ page, setPage }: { page: Page; setPage: (p: Page) => void }) {
  const { t, lang } = useT();
  const { updateSettings } = useApp();
  return (
    <nav aria-label="Sections" className="flex w-[220px] shrink-0 flex-col bg-[var(--sidebar)] px-3 pb-3 pt-2">
      <ul className="flex flex-col gap-0.5">
        {NAV.map(({ id, icon: Icon }) => (
          <li key={id}>
            <button
              onClick={() => setPage(id)}
              aria-current={page === id ? "page" : undefined}
              className={`relative flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-lg px-3 text-[14px] transition-colors ${
                page === id ? "font-semibold text-[var(--label)]" : "text-[var(--label)] hover:bg-[var(--fill)]"
              }`}
            >
              {page === id && <motion.span layoutId="nav-pill" transition={spring} className="absolute inset-0 rounded-lg bg-[var(--fill-hover)]" />}
              <Icon size={17} strokeWidth={1.8} className="relative text-[var(--link)]" />
              <span className="relative">{t(`nav.${id}` as Key)}</span>
            </button>
          </li>
        ))}
      </ul>
      <div className="mt-auto flex rounded-lg bg-[var(--fill)] p-0.5 text-[12px] font-semibold" role="radiogroup" aria-label={t("settings.language")}>
        {(["en", "ru"] as const).map((l) => (
          <button
            key={l}
            role="radio"
            aria-checked={lang === l}
            onClick={() => updateSettings({ lang: l })}
            className={`flex-1 cursor-pointer rounded-md py-1 ${lang === l ? "bg-[var(--card)] text-[var(--label)] shadow-sm" : "text-[var(--secondary)]"}`}
          >
            {l.toUpperCase()}
          </button>
        ))}
      </div>
    </nav>
  );
}

function CommandPalette({ close }: { close: () => void }) {
  const { t, lang } = useT();
  const { items, go, installed } = useApp();
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  const results = useMemo(() => {
    const ql = q.trim().toLowerCase();
    const r: { key: string; label: string; hint: string; run: () => void }[] = [];
    for (const n of NAV) {
      const label = t(`nav.${n.id}` as Key);
      if (!ql || label.toLowerCase().includes(ql)) r.push({ key: `nav-${n.id}`, label, hint: t("palette.go"), run: () => go(n.id) });
    }
    for (const it of items) {
      if (!ql || it.name.toLowerCase().includes(ql) || it.description[lang].toLowerCase().includes(ql))
        r.push({
          key: it.id,
          label: it.name,
          hint: installed[it.id] !== undefined ? t("btn.installed") : t("palette.install"),
          run: () => (go("apps"), installed[it.id] === undefined && api.install([it.id])),
        });
    }
    for (const id of ["mouse_speed", "mouse_precision", "power_plan"] as const) {
      const label = t(`tweaks.${id}.title` as Key);
      if (ql && label.toLowerCase().includes(ql)) r.push({ key: id, label, hint: t("nav.tweaks"), run: () => go("tweaks") });
    }
    return r.slice(0, 12);
  }, [q, items, installed, lang, t, go]);

  const run = (i: number) => {
    results[i]?.run();
    close();
  };

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className="fixed inset-0 z-50 flex justify-center bg-black/20 pt-[12vh]"
      onMouseDown={close}
    >
      <motion.div
        initial={{ scale: 0.97, y: -8 }}
        animate={{ scale: 1, y: 0 }}
        exit={{ scale: 0.97, y: -8 }}
        transition={spring}
        role="dialog"
        aria-modal
        aria-label={t("common.search")}
        onMouseDown={(e) => e.stopPropagation()}
        className="h-fit w-[min(600px,90vw)] overflow-hidden rounded-2xl border border-[var(--card-border)] bg-[var(--sheet)] backdrop-blur-xl"
        style={{ boxShadow: "0 24px 80px rgba(0,0,0,.3)" }}
      >
        <div className="flex items-center gap-3 border-b border-[var(--separator)] px-4">
          <Search size={18} className="text-[var(--secondary)]" />
          <input
            ref={input}
            value={q}
            onChange={(e) => (setQ(e.target.value), setSel(0))}
            onKeyDown={(e) => {
              if (e.key === "Escape") close();
              if (e.key === "ArrowDown") (e.preventDefault(), setSel((s) => Math.min(s + 1, results.length - 1)));
              if (e.key === "ArrowUp") (e.preventDefault(), setSel((s) => Math.max(s - 1, 0)));
              if (e.key === "Enter") run(sel);
            }}
            placeholder={t("palette.placeholder")}
            aria-label={t("palette.placeholder")}
            className="h-14 flex-1 bg-transparent text-[17px] outline-none placeholder:text-[var(--tertiary)]"
          />
        </div>
        <ul role="listbox" className="max-h-[50vh] overflow-auto p-2">
          {results.length === 0 && <li className="px-3 py-6 text-center text-[var(--secondary)]">{t("palette.empty")}</li>}
          {results.map((r, i) => (
            <li
              key={r.key}
              role="option"
              aria-selected={i === sel}
              onMouseEnter={() => setSel(i)}
              onClick={() => run(i)}
              className={`flex cursor-pointer items-center justify-between rounded-lg px-3 py-2 ${i === sel ? "bg-[var(--accent)] text-[var(--accent-text)]" : ""}`}
            >
              <span className="font-medium">{r.label}</span>
              <span className={`text-[12px] ${i === sel ? "opacity-90" : "text-[var(--secondary)]"}`}>{r.hint}</span>
            </li>
          ))}
        </ul>
      </motion.div>
    </motion.div>
  );
}
