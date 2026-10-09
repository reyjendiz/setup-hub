import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AppWindow, File, FileArchive, FileImage, FileText, Folder, FolderOpen, Plus, RefreshCw, X } from "lucide-react";
import { FormEvent, useEffect, useState } from "react";
import { api, DriveEntry, DriveFolder, fmtSize, useJob } from "../api";
import { useApp } from "../App";
import { Button, Card, JobButton, PageHeader } from "../components";
import { useT } from "../i18n";

function TypeIcon({ e }: { e: DriveEntry }) {
  const n = e.name.toLowerCase();
  const Icon = e.is_folder
    ? Folder
    : /\.(zip|rar|7z|tar|gz)$/.test(n)
      ? FileArchive
      : /\.(exe|msi)$/.test(n)
        ? AppWindow
        : /image\//.test(e.mime)
          ? FileImage
          : /text|pdf|document/.test(e.mime)
            ? FileText
            : File;
  return <Icon size={20} strokeWidth={1.7} className={e.is_folder ? "text-[var(--link)]" : "text-[var(--secondary)]"} aria-hidden />;
}

export default function Files() {
  const { t } = useT();
  const { settings, updateSettings, boot } = useApp();
  const folders = settings.drive_folders;
  const [current, setCurrent] = useState<string | null>(folders[0]?.id ?? null);
  const [entries, setEntries] = useState<DriveEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const folder = folders.find((f) => f.id === current) ?? folders[0];

  const load = () => {
    if (!folder) return;
    setEntries(null);
    setError(null);
    api.driveList(folder.id).then(setEntries, (e) => setError(String(e)));
  };
  useEffect(load, [folder?.id]);

  const files = entries?.filter((e) => !e.is_folder) ?? [];
  // Each folder gets its own subfolder, so two folders' files never mix.
  const dest = folder ? `${settings.drive_dest}\\${folder.name.replace(/[<>:"/\\|?*]/g, "_").replace(/^\.+|\.+$/g, "_")}` : settings.drive_dest;
  const download = (list: DriveEntry[]) => api.driveDownload(list.map(({ id, name, path }) => ({ id, name, path })), dest);

  const add = (f: DriveFolder) => {
    if (!folders.some((x) => x.id === f.id)) updateSettings({ drive_folders: [...folders, f] });
    setCurrent(f.id);
  };
  const remove = (id: string) => {
    updateSettings({ drive_folders: folders.filter((f) => f.id !== id) });
    if (current === id) setCurrent(folders.find((f) => f.id !== id)?.id ?? null);
  };

  const pick = async () => {
    const d = await open({ directory: true, defaultPath: settings.drive_dest });
    if (typeof d === "string") updateSettings({ drive_dest: d });
  };

  return (
    <>
      <PageHeader title={t("files.title")} subtitle={t("files.subtitle")}>
        {folder && (
          <>
            <Button variant="plain" onClick={load} aria-label={t("common.refresh")}>
              <RefreshCw size={14} /> {t("common.refresh")}
            </Button>
            <Button variant="primary" className="h-10 px-5 text-[15px]" disabled={!files.length} onClick={() => download(files)}>
              {t("files.downloadAll")}
              {files.length > 0 && <span className="font-normal opacity-80">· {fmtSize(files.reduce((a, f) => a + (f.size ?? 0), 0))}</span>}
            </Button>
          </>
        )}
      </PageHeader>

      {folders.length > 0 && (
        <div className="mb-4 flex flex-wrap items-center gap-2" role="tablist" aria-label={t("files.folders")}>
          {folders.map((f) => (
            <span
              key={f.id}
              className={`flex h-8 items-center rounded-full text-[13px] font-medium ${f.id === folder?.id ? "bg-[var(--accent)] text-white" : "bg-[var(--fill)]"}`}
            >
              <button role="tab" aria-selected={f.id === folder?.id} className="flex h-full cursor-pointer items-center gap-1.5 pl-3 pr-1" onClick={() => setCurrent(f.id)}>
                <Folder size={14} /> {f.name}
              </button>
              <button
                className="grid h-6 w-6 cursor-pointer place-items-center rounded-full opacity-70 hover:opacity-100"
                aria-label={t("files.remove", { name: f.name })}
                title={t("files.remove", { name: f.name })}
                onClick={() => remove(f.id)}
              >
                <X size={13} />
              </button>
            </span>
          ))}
        </div>
      )}

      <AddFolder onAdd={add} first={folders.length === 0} />

      {folder && (
        <>
          <Card lift={false} className="mb-5 flex items-center gap-3 px-4 py-3">
            <FolderOpen size={18} className="text-[var(--link)]" />
            <div className="min-w-0 flex-1">
              <div className="caption">{t("files.destination")}</div>
              <div className="selectable truncate text-[14px]">{dest}</div>
            </div>
            <Button onClick={pick}>{t("files.change")}</Button>
            <Button variant="plain" onClick={() => revealItemInDir(settings.drive_dest).catch(() => {})}>{t("files.showInExplorer")}</Button>
          </Card>

          {!boot.has_google_key && <p className="mb-3 text-[13px] text-[var(--secondary)]">{t("files.noKey")}</p>}

          <Card lift={false} className="overflow-hidden">
            {error && <p className="selectable p-6 text-[var(--red)]">{error}</p>}
            {!entries && !error && (
              <ul aria-busy className="divide-y divide-[var(--separator)]">
                {Array.from({ length: 6 }).map((_, i) => (
                  <li key={i} className="flex items-center gap-3 px-4 py-3">
                    <div className="h-5 w-5 animate-pulse rounded bg-[var(--fill)]" />
                    <div className="h-3.5 animate-pulse rounded bg-[var(--fill)]" style={{ width: `${30 + ((i * 17) % 40)}%` }} />
                  </li>
                ))}
              </ul>
            )}
            {entries?.length === 0 && <p className="p-6 text-center text-[var(--secondary)]">{t("files.empty")}</p>}
            {entries && entries.length > 0 && (
              <ul className="divide-y divide-[var(--separator)]">
                {entries.map((e) => (
                  <FileRow key={e.id} e={e} onDownload={() => download([e])} />
                ))}
              </ul>
            )}
          </Card>
        </>
      )}
    </>
  );
}

/** Paste a Google Drive folder link; it's checked (public, really a folder) before it's saved. */
function AddFolder({ onAdd, first }: { onAdd: (f: DriveFolder) => void; first: boolean }) {
  const { t } = useT();
  const [link, setLink] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!link.trim()) return;
    setBusy(true);
    setError(null);
    try {
      onAdd(await api.driveFolderInfo(link.trim()));
      setLink("");
    } catch (err) {
      setError(String(err));
    }
    setBusy(false);
  };

  return (
    <Card lift={false} className={`mb-5 px-4 ${first ? "py-6" : "py-3"}`}>
      {first && (
        <div className="mb-3">
          <div className="section-title">{t("files.addTitle")}</div>
          <p className="text-[13px] text-[var(--secondary)]">{t("files.addHint")}</p>
        </div>
      )}
      <form onSubmit={submit} className="flex gap-2">
        <input
          className="field flex-1"
          placeholder="https://drive.google.com/drive/folders/…"
          aria-label={t("files.addTitle")}
          value={link}
          onChange={(e) => setLink(e.target.value)}
        />
        <Button variant={first ? "primary" : "secondary"} type="submit" disabled={busy || !link.trim()}>
          <Plus size={15} /> {t("files.add")}
        </Button>
      </form>
      {error && <p className="selectable mt-2 text-[12px] text-[var(--red)]">{error}</p>}
    </Card>
  );
}

function FileRow({ e, onDownload }: { e: DriveEntry; onDownload: () => void }) {
  const { t } = useT();
  const job = useJob(`drive:${e.id}`);
  const depth = e.path ? e.path.split("/").length : 0;
  return (
    <li className="flex items-center gap-3 px-4 py-2.5" style={{ paddingLeft: 16 + depth * 20 }}>
      <TypeIcon e={e} />
      <span className={`min-w-0 flex-1 truncate text-[14px] ${e.is_folder ? "font-semibold" : ""}`} title={e.name}>
        {e.name}
      </span>
      {!e.is_folder && (
        <>
          <span className="w-20 text-right text-[13px] tabular-nums text-[var(--secondary)]">{fmtSize(e.size)}</span>
          {job?.phase === "done" && job.message && (
            <Button variant="plain" className="min-w-0 px-2" onClick={() => revealItemInDir(job.message!)}>
              {t("files.showInExplorer")}
            </Button>
          )}
          <JobButton
            job={job}
            done={false}
            idleLabel={t("btn.download")}
            doneLabel={t("btn.downloaded")}
            redoLabel={t("btn.download")}
            onStart={onDownload}
            onCancel={() => api.cancel(`drive:${e.id}`)}
          />
        </>
      )}
    </li>
  );
}
