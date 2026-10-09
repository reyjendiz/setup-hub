import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { AnimatePresence, motion } from "framer-motion";
import { ChevronUp, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, getJob, getLog, isActive, Item, useJob, useJobsVersion } from "../api";
import { useApp } from "../App";
import { AppIcon, Badge, Button, Card, JobButton, PageHeader, Segmented, spring } from "../components";
import { Key, useT } from "../i18n";

type Tab = "programs" | "games";

/** Sections of the Programs tab, by catalog category. */
const GROUPS: { id: string; title: Key; hint: Key }[] = [
  { id: "everyday", title: "apps.groupEveryday", hint: "apps.groupEverydayHint" },
  { id: "design", title: "apps.groupDesign", hint: "apps.groupDesignHint" },
  { id: "coding", title: "apps.groupCoding", hint: "apps.groupCodingHint" },
  { id: "dev", title: "apps.devTitle", hint: "apps.devHint" },
];
const groupOf = (i: Item) => (i.category === "creative" ? "design" : i.category === "coding" || i.category === "dev" ? i.category : "everyday");
const tabOf = (i: Item): Tab => (i.category === "gaming" ? "games" : "programs");

function savedTab(): Tab {
  try {
    return localStorage.getItem("apps.tab") === "games" ? "games" : "programs";
  } catch {
    return "programs";
  }
}

/** Messages the engine sends after making an app the default (see defaults.rs after_install). */
const DEFAULT_MSG: Record<string, Key> = {
  "default-signin": "apps.default.signin",
  "default-signin-home": "apps.default.signinHome",
  "default-settings": "apps.default.settings",
  "default-print-screen": "apps.default.printScreen",
};

export default function Apps() {
  const { t, lang } = useT();
  const { items, installed, search, setSearch, boot, myApps, appUpdates } = useApp();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [batch, setBatch] = useState<string[]>([]);
  useJobsVersion(); // re-render on any job event for the batch bar

  const [tab, setTab] = useState<Tab>(savedTab);
  useEffect(() => {
    try {
      localStorage.setItem("apps.tab", tab);
    } catch {}
  }, [tab]);

  const q = search.trim().toLowerCase();
  const matching = items.filter((i) => !q || i.name.toLowerCase().includes(q) || i.description[lang].toLowerCase().includes(q));
  const count = (tb: Tab) => matching.filter((i) => tabOf(i) === tb).length;
  const shown = matching.filter((i) => tabOf(i) === tab);
  // Programs are grouped by who they're for; each group can be installed in one go. Games stay one grid.
  const groups =
    tab === "games"
      ? [{ id: "games", items: shown }]
      : GROUPS.map((g) => ({ ...g, items: shown.filter((i) => groupOf(i) === g.id) })).filter((g) => g.items.length > 0);

  const start = (ids: string[]) => {
    // Interactive installers go last so the silent ones finish without waiting on a click.
    const interactive = (id: string) => Number(items.find((i) => i.id === id)?.interactive ?? myApps.find((m) => m.id === id)?.mode === "interactive");
    const ordered = [...ids].sort((a, b) => interactive(a) - interactive(b));
    setBatch(ordered);
    api.install(ordered);
    setSelected(new Set());
  };
  const notInstalled = [
    ...items.filter((i) => installed[i.id] === undefined).map((i) => i.id),
    // My Apps entries opted into "Install all" (download-only ones aren't installs).
    ...myApps.filter((m) => m.include_in_install_all && m.mode !== "download_only" && installed[m.id] === undefined).map((m) => m.id),
  ];

  const finished = batch.filter((id) => ["done", "failed", "cancelled"].includes(getJob(id)?.phase ?? "")).length;
  const partial = batch.reduce((a, id) => {
    const j = getJob(id);
    return a + (j?.phase === "downloading" && j.progress ? j.progress / 100 / 2 : j?.phase === "installing" || j?.phase === "verifying" ? 0.5 : 0);
  }, 0);
  const running = batch.length > 0 && batch.some((id) => isActive(getJob(id)));

  return (
    <>
      <PageHeader title={t("apps.title")} subtitle={t("apps.subtitle")}>
        {selected.size > 0 ? (
          <>
            <Button variant="plain" onClick={() => setSelected(new Set())}>{t("apps.clearSelection")}</Button>
            <Button variant="primary" className="h-10 px-5 text-[15px]" onClick={() => start([...selected])}>
              {t("apps.installSelected", { n: selected.size })}
            </Button>
          </>
        ) : (
          <>
          {Object.keys(appUpdates).length > 0 && (
            <Button className="h-10 px-5 text-[15px]" disabled={running} onClick={() => start(Object.keys(appUpdates))}>
              {t("apps.updateAll", { n: Object.keys(appUpdates).length })}
            </Button>
          )}
          <Button
            variant="primary"
            className="h-10 px-5 text-[15px]"
            disabled={notInstalled.length === 0 || running}
            title={notInstalled.length === 0 ? t("apps.allInstalled") : undefined}
            onClick={() => start(notInstalled)}
          >
            {t("apps.installAll")}
          </Button>
          </>
        )}
      </PageHeader>

      <AnimatePresence>
        {batch.length > 0 && (running || finished < batch.length) && (
          <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} className="mb-5">
            <div className="mb-1.5 flex justify-between text-[13px] text-[var(--secondary)]">
              <span>{t("apps.progress", { done: finished, total: batch.length })}</span>
              <span className="tabular-nums">{Math.round(((finished + partial) / batch.length) * 100)}%</span>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-[var(--fill)]" role="progressbar" aria-valuenow={finished} aria-valuemax={batch.length}>
              <motion.div className="h-full rounded-full bg-[var(--accent)]" animate={{ width: `${((finished + partial) / batch.length) * 100}%` }} transition={spring} />
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <div className="mb-5 flex flex-wrap items-center gap-3">
        <Segmented
          label={t("apps.tabs")}
          value={tab}
          onChange={setTab}
          options={[
            ["programs", `${t("apps.tabPrograms")} · ${count("programs")}`],
            ["games", `${t("apps.tabGames")} · ${count("games")}`],
          ]}
        />
        <div className="relative w-full max-w-sm">
          <Search size={15} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--secondary)]" />
          <input className="field pl-9" placeholder={t("common.search")} aria-label={t("common.search")} value={search} onChange={(e) => setSearch(e.target.value)} />
        </div>
      </div>

      {shown.length === 0 ? (
        <p className="py-16 text-center text-[var(--secondary)]">{q ? t("apps.empty", { q: search }) : t("apps.emptyTab")}</p>
      ) : (
        groups.map((g) => {
          const missing = g.items.filter((i) => installed[i.id] === undefined).map((i) => i.id);
          return (
            <section key={g.id} className="mb-8">
              {"title" in g && (
                <div className="mb-3 flex flex-wrap items-end justify-between gap-3">
                  <div>
                    <h2 className="section-title">{t(g.title)}</h2>
                    <p className="text-[13px] text-[var(--secondary)]">{t(g.hint)}</p>
                  </div>
                  <Button disabled={missing.length === 0 || running} onClick={() => start(missing)}>
                    {missing.length ? t("apps.installGroup", { n: missing.length }) : t("apps.allInstalled")}
                  </Button>
                </div>
              )}
              <AppGrid items={g.items} selected={selected} setSelected={setSelected} />
            </section>
          );
        })
      )}
      <p className="caption mb-20 mt-6">{t("apps.catalogOrigin", { origin: boot.catalog_origin })}</p>
      <LogSheet />
    </>
  );
}

function AppGrid({ items, selected, setSelected }: { items: Item[]; selected: Set<string>; setSelected: (f: (s: Set<string>) => Set<string>) => void }) {
  const { installed } = useApp();
  if (items.length === 0) return null;
  return (
    <motion.div layout className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4">
      {items.map((it) => (
        <AppCard
          key={it.id}
          item={it}
          installed={installed[it.id]}
          selected={selected.has(it.id)}
          selecting={selected.size > 0}
          onSelect={(v) => setSelected((s) => { const n = new Set(s); v ? n.add(it.id) : n.delete(it.id); return n; })}
        />
      ))}
    </motion.div>
  );
}

function AppCard({ item, installed, selected, selecting, onSelect }: { item: Item; installed?: string; selected: boolean; selecting: boolean; onSelect: (v: boolean) => void }) {
  const { t, lang } = useT();
  const job = useJob(item.id);
  const upd = useApp().appUpdates[item.id];
  const waitingOnUser = item.interactive && job?.phase === "installing";
  const defaultMsg = job?.phase === "done" ? DEFAULT_MSG[job.message ?? ""] : undefined;
  return (
    <Card className="group relative flex flex-col gap-3 p-4">
      <div className="flex items-start gap-3">
        <label className={`absolute left-2 top-2 z-10 transition-opacity ${selecting || selected ? "opacity-100" : "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"}`}>
          <input
            type="checkbox"
            checked={selected}
            onChange={(e) => onSelect(e.target.checked)}
            aria-label={`Select ${item.name}`}
            className="h-4 w-4 cursor-pointer accent-[var(--accent)]"
          />
        </label>
        <AppIcon id={item.id} name={item.name} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[15px] font-semibold">{item.name}</div>
          <div className="line-clamp-2 text-[13px] text-[var(--secondary)]">{item.description[lang]}</div>
        </div>
      </div>
      <div className="mt-auto flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap gap-1">
          {item.interactive && <Badge tone="orange">{t("apps.needsInteraction")}</Badge>}
          {item.unelevated && <Badge>{t("apps.nonAdmin")}</Badge>}
          {upd ? <Badge tone="orange">{upd.installed} → {upd.available}</Badge> : installed && <Badge tone="green">{installed}</Badge>}
        </div>
        <JobButton
          job={job}
          done={installed !== undefined && !upd}
          idleLabel={upd ? t("apps.update") : undefined}
          onStart={() => api.install([item.id])}
          onCancel={() => api.cancel(item.id)}
        />
      </div>
      {(waitingOnUser || (item.note && job?.phase !== "done")) && (
        <p className={`text-[12px] ${waitingOnUser ? "font-medium text-[var(--orange)]" : "text-[var(--tertiary)]"}`}>
          {waitingOnUser ? t("apps.interactiveRunning") : item.note?.[lang]}
        </p>
      )}
      {defaultMsg && (
        <div className="flex items-center justify-between gap-2">
          <p className="text-[12px] text-[var(--secondary)]">{t(defaultMsg, { name: item.name })}</p>
          {(job?.message === "default-settings" || job?.message === "default-signin-home") && (
            <Button
              variant="plain"
              className="shrink-0"
              onClick={() => (item.print_screen ? openUrl("ms-settings:easeofaccess-keyboard") : api.defaultsSettingsUri(item.id).then(openUrl))}
            >
              {t("tweaks.openSettings")}
            </Button>
          )}
        </div>
      )}
    </Card>
  );
}

function LogSheet() {
  const { t } = useT();
  const { items, myApps } = useApp();
  const [open, setOpen] = useState(false);
  const v = useJobsVersion();
  const lines = useMemo(() => [...getLog()].reverse(), [v]);
  const name = (id: string) => items.find((i) => i.id === id)?.name ?? myApps.find((m) => m.id === id.replace(/^uninstall:/, ""))?.name ?? id;
  return (
    <div className="pointer-events-none fixed bottom-0 left-[220px] right-0 z-20 flex justify-center px-8 pb-4">
      <motion.div
        layout
        transition={spring}
        className="pointer-events-auto w-full max-w-3xl overflow-hidden rounded-2xl border border-[var(--card-border)] bg-[var(--sheet)] backdrop-blur-xl"
        style={{ boxShadow: "var(--shadow-hover)" }}
      >
        <button onClick={() => setOpen((o) => !o)} aria-expanded={open} className="flex w-full cursor-pointer items-center justify-between px-4 py-2.5 text-[13px] font-semibold">
          <span>
            {t("apps.log")}
            {lines[0] && !open && <span className="ml-2 font-normal text-[var(--secondary)]">{name(lines[0].id)} · {lines[0].phase}</span>}
          </span>
          <motion.span animate={{ rotate: open ? 180 : 0 }} transition={spring}>
            <ChevronUp size={16} />
          </motion.span>
        </button>
        <AnimatePresence initial={false}>
          {open && (
            <motion.div initial={{ height: 0 }} animate={{ height: 260 }} exit={{ height: 0 }} transition={spring} className="flex flex-col border-t border-[var(--separator)]">
              <ul className="selectable flex-1 overflow-auto px-4 py-2 font-mono text-[12px]">
                {lines.length === 0 && <li className="py-4 text-center font-sans text-[var(--secondary)]">{t("apps.logEmpty")}</li>}
                {lines.map((l, i) => (
                  <li key={i} className="flex gap-3 py-0.5">
                    <span className="text-[var(--tertiary)]">{l.t.toLocaleTimeString()}</span>
                    <span className="w-40 shrink-0 truncate font-semibold">{name(l.id)}</span>
                    <span className={l.phase === "failed" ? "text-[var(--red)]" : l.phase === "done" ? "text-[var(--green)]" : "text-[var(--secondary)]"}>
                      {l.phase}
                      {l.message ? ` — ${l.message}` : ""}
                    </span>
                  </li>
                ))}
              </ul>
              <div className="flex justify-end border-t border-[var(--separator)] px-3 py-2">
                <Button variant="plain" onClick={() => api.logsDir().then((d) => revealItemInDir(d))}>{t("apps.openLogs")}</Button>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    </div>
  );
}
