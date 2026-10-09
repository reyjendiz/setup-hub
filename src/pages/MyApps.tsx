import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Ellipsis, Link2, Plus, RefreshCw } from "lucide-react";
import { useState } from "react";
import { api, isActive, isNewer, MyApp, sourceLabel, useJob } from "../api";
import { useApp } from "../App";
import { AppIcon, Badge, Button, Card, JobButton, Menu, MenuItem, PageHeader, ProgressRing } from "../components";
import { Key, useT } from "../i18n";

type Status = Record<string, { error?: string | null; latest?: string | null }>;

export default function MyApps() {
  const { t } = useT();
  const { myApps, openAdd } = useApp();
  const [status, setStatus] = useState<Status>({});
  const [checking, setChecking] = useState(false);
  const [checkMsg, setCheckMsg] = useState<string | null>(null);
  const [order, setOrder] = useState<string[] | null>(null); // live order while dragging
  const [dragId, setDragId] = useState<string | null>(null);

  const list = order ? order.map((id) => myApps.find((a) => a.id === id)!).filter(Boolean) : myApps;

  const check = async () => {
    setChecking(true);
    setCheckMsg(null);
    try {
      const s = await api.myappsCheck();
      setStatus(s);
      const broken = Object.values(s).filter((x) => x.error).length;
      setCheckMsg(broken ? t("my.checkBroken", { n: broken }) : t("my.checkOk"));
    } catch (e) {
      setCheckMsg(String(e));
    }
    setChecking(false);
  };

  const move = (id: string, by: number) => {
    const ids = myApps.map((a) => a.id);
    const i = ids.indexOf(id);
    const j = Math.max(0, Math.min(ids.length - 1, i + by));
    ids.splice(j, 0, ids.splice(i, 1)[0]);
    api.myappsReorder(ids);
  };

  return (
    <>
      <PageHeader title={t("my.title")} subtitle={t("my.subtitle")}>
        {myApps.length > 0 && (
          <Button variant="plain" onClick={check} disabled={checking}>
            {checking ? <ProgressRing /> : <RefreshCw size={14} />} {checking ? t("my.checking") : t("my.check")}
          </Button>
        )}
        <Button variant="primary" className="h-10 px-5 text-[15px]" onClick={() => openAdd()}>
          <Plus size={16} strokeWidth={2.5} /> {t("top.add")}
        </Button>
      </PageHeader>

      {checkMsg && <p className="mb-4 text-[13px] text-[var(--secondary)]">{checkMsg}</p>}

      {myApps.length === 0 ? (
        <Card lift={false} className="flex flex-col items-center gap-3 px-6 py-14 text-center">
          <div className="grid h-14 w-14 place-items-center rounded-2xl bg-[var(--fill)] text-[var(--link)]">
            <Link2 size={26} />
          </div>
          <div className="section-title">{t("my.emptyTitle")}</div>
          <p className="max-w-md text-[14px] text-[var(--secondary)]">{t("my.emptyBody")}</p>
          <Button variant="primary" className="mt-2 h-10 px-5 text-[15px]" onClick={() => openAdd()}>
            <Plus size={16} strokeWidth={2.5} /> {t("top.add")}
          </Button>
        </Card>
      ) : (
        <>
          <p className="caption mb-3">{t("my.dragHint")}</p>
          <div
            className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-4 pb-24"
            onDragEnd={() => {
              if (order) api.myappsReorder(order);
              setOrder(null);
              setDragId(null);
            }}
          >
            {list.map((a, i) => (
              <div
                key={a.id}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.effectAllowed = "move";
                  setDragId(a.id);
                  setOrder(myApps.map((x) => x.id));
                }}
                onDragOver={(e) => {
                  e.preventDefault();
                  if (!dragId || dragId === a.id || !order) return;
                  const ids = [...order];
                  ids.splice(ids.indexOf(a.id), 0, ids.splice(ids.indexOf(dragId), 1)[0]);
                  setOrder(ids);
                }}
                className={dragId === a.id ? "opacity-50" : ""}
              >
                <MyAppCard app={a} status={status[a.id]} first={i === 0} last={i === list.length - 1} onMove={(by) => move(a.id, by)} />
              </div>
            ))}
          </div>
        </>
      )}
    </>
  );
}

function MyAppCard({ app, status, first, last, onMove }: { app: MyApp; status?: Status[string]; first: boolean; last: boolean; onMove: (by: number) => void }) {
  const { t } = useT();
  const { installed, openAdd, openEntry } = useApp();
  const job = useJob(app.id);
  const unJob = useJob(`uninstall:${app.id}`);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const inst = installed[app.id];
  const update = status?.latest && inst && isNewer(status.latest, inst) ? status.latest : null;

  const items: MenuItem[] = [
    { label: t("my.menu.edit"), onSelect: () => openEntry(app, false) },
    { label: t("my.menu.changeLink"), onSelect: () => openAdd(app) },
    { label: t("my.menu.duplicate"), onSelect: () => api.myappsDuplicate(app.id) },
    { label: t("my.menu.includeAll"), checked: app.include_in_install_all, onSelect: () => api.myappsUpdate({ ...app, include_in_install_all: !app.include_in_install_all }) },
    { separator: true, label: "", onSelect: () => {} },
    { label: t("my.menu.moveUp"), disabled: first, onSelect: () => onMove(-1) },
    { label: t("my.menu.moveDown"), disabled: last, onSelect: () => onMove(1) },
    { separator: true, label: "", onSelect: () => {} },
    { label: t("my.menu.uninstall"), disabled: inst === undefined || isActive(unJob), danger: true, onSelect: () => api.myappsUninstall(app.id) },
    { label: t("my.menu.remove"), danger: true, onSelect: () => confirm(t("my.removeConfirm", { name: app.name })) && api.myappsRemove(app.id) },
  ];

  return (
    <Card className="group flex h-full flex-col gap-3 p-4">
      <div
        className="flex items-start gap-3"
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
      >
        <div className="relative">
          <AppIcon id={app.id} name={app.name} src={app.icon} />
          {status?.error && <span className="absolute -right-1 -top-1 h-3 w-3 rounded-full border-2 border-[var(--card)] bg-[var(--red)]" title={status.error} aria-label={t("my.broken")} />}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[15px] font-semibold">{app.name}</div>
          <div className="truncate text-[12px] text-[var(--secondary)]" title={sourceLabel(app.source)}>
            {app.publisher ?? sourceLabel(app.source)}
          </div>
        </div>
        <button
          className="cursor-pointer rounded-full p-1 text-[var(--secondary)] opacity-60 hover:bg-[var(--fill)] hover:opacity-100 focus-visible:opacity-100"
          aria-label={t("my.menu.more", { name: app.name })}
          aria-haspopup="menu"
          onClick={(e) => {
            const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
            setMenu({ x: r.left, y: r.bottom + 4 });
          }}
        >
          <Ellipsis size={18} />
        </button>
      </div>

      {status?.error && (
        <div className="flex items-center justify-between gap-2 rounded-lg bg-[var(--fill)] px-2.5 py-1.5 text-[12px]">
          <span className="truncate text-[var(--red)]" title={status.error}>{t("my.broken")}: {status.error}</span>
          <Button variant="plain" className="h-6 min-w-0 px-2" onClick={() => openAdd(app)}>{t("my.menu.changeLink")}</Button>
        </div>
      )}

      <div className="mt-auto flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap gap-1">
          {app.mode !== "silent" && <Badge tone={app.mode === "interactive" ? "orange" : "neutral"}>{t(`my.mode.${app.mode}` as Key)}</Badge>}
          {!app.include_in_install_all && <Badge>{t("my.notInAll")}</Badge>}
          {inst && <Badge tone="green">{inst}</Badge>}
          {update && <Badge tone="orange">{t("my.update", { v: update })}</Badge>}
          {isActive(unJob) && <Badge tone="red">{t("my.uninstalling")}</Badge>}
        </div>
        {job?.phase === "done" && app.mode === "download_only" && job.message ? (
          <Button variant="plain" onClick={() => revealItemInDir(job.message!)}>{t("files.showInExplorer")}</Button>
        ) : (
          <JobButton
            job={job}
            done={inst !== undefined && !update}
            idleLabel={app.mode === "download_only" ? t("btn.download") : update ? t("btn.reinstall") : undefined}
            onStart={() => api.install([app.id])}
            onCancel={() => api.cancel(app.id)}
            onReview={() => openEntry(app, true)}
          />
        )}
      </div>
      {menu && <Menu at={menu} items={items} onClose={() => setMenu(null)} />}
    </Card>
  );
}
