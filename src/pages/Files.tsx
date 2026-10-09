import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AppWindow, File, FileArchive, FileImage, FileText, Folder, FolderOpen, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { api, DriveEntry, fmtSize, useJob } from "../api";
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
  const [entries, setEntries] = useState<DriveEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    setEntries(null);
    setError(null);
    api.driveList().then(setEntries, (e) => setError(String(e)));
  };
  useEffect(load, []);

  const files = entries?.filter((e) => !e.is_folder) ?? [];
  const download = (list: DriveEntry[]) => api.driveDownload(list.map(({ id, name, path }) => ({ id, name, path })), settings.drive_dest);

  const pick = async () => {
    const d = await open({ directory: true, defaultPath: settings.drive_dest });
    if (typeof d === "string") updateSettings({ drive_dest: d });
  };

  return (
    <>
      <PageHeader title={t("files.title")} subtitle={t("files.subtitle")}>
        <Button variant="plain" onClick={load} aria-label={t("common.refresh")}>
          <RefreshCw size={14} /> {t("common.refresh")}
        </Button>
        <Button variant="primary" className="h-10 px-5 text-[15px]" disabled={!files.length} onClick={() => download(files)}>
          {t("files.downloadAll")}
          {files.length > 0 && <span className="font-normal opacity-80">· {fmtSize(files.reduce((a, f) => a + (f.size ?? 0), 0))}</span>}
        </Button>
      </PageHeader>

      <Card lift={false} className="mb-5 flex items-center gap-3 px-4 py-3">
        <FolderOpen size={18} className="text-[var(--link)]" />
        <div className="min-w-0 flex-1">
          <div className="caption">{t("files.destination")}</div>
          <div className="selectable truncate text-[14px]">{settings.drive_dest}</div>
        </div>
        <Button onClick={pick}>{t("files.change")}</Button>
        <Button variant="plain" onClick={() => revealItemInDir(settings.drive_dest).catch(() => {})}>{t("files.showInExplorer")}</Button>
      </Card>

      {!boot.has_google_key && <p className="mb-3 text-[13px] text-[var(--secondary)]">{t("files.noKey")}</p>}

      <Card lift={false} className="overflow-hidden">
        {error && <p className="p-6 text-[var(--red)]">{error}</p>}
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
