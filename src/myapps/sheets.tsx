import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, ChevronRight, ClipboardPaste, ShieldAlert, ShieldCheck, Archive } from "lucide-react";
import { useEffect, useState } from "react";
import { Analysis, api, fmtSize, Mode, MyApp, sourceLabel, useJob } from "../api";
import { useApp } from "../App";
import { AppIcon, Badge, Button, Field, ProgressRing, Sheet, Toggle } from "../components";
import { Key, useT } from "../i18n";

export const MODES: Mode[] = ["silent", "interactive", "download_only", "extract"];
export const splitArgs = (s: string) => s.match(/(?:[^\s"]+|"[^"]*")+/g) ?? [];

/** Every auto-detected value stays editable: name, mode, silent args, extract target, Install-all membership. */
export function EntryFields({ entry, onChange, compact }: { entry: MyApp; onChange: (e: MyApp) => void; compact?: boolean }) {
  const { t } = useT();
  const [args, setArgs] = useState(entry.silent_args.join(" "));
  useEffect(() => setArgs(entry.silent_args.join(" ")), [entry.silent_args.join(" ")]);
  const isWinget = entry.source.type === "winget";
  return (
    <div className="flex flex-col gap-3">
      <Field label={t("add.name")}>
        <input className="field" value={entry.name} maxLength={120} onChange={(e) => onChange({ ...entry, name: e.target.value })} />
      </Field>
      {!isWinget && (
        <Field label={t("add.mode")}>
          <select className="field" value={entry.mode} onChange={(e) => onChange({ ...entry, mode: e.target.value as Mode })}>
            {MODES.map((m) => (
              <option key={m} value={m}>
                {t(`my.mode.${m}` as Key)}
              </option>
            ))}
          </select>
        </Field>
      )}
      {entry.mode === "extract" && (
        <>
          <Field label={t("add.extractTo")}>
            <input className="field" value={entry.extract_to ?? ""} placeholder={`%LOCALAPPDATA%\\Programs\\${entry.name}`} onChange={(e) => onChange({ ...entry, extract_to: e.target.value || null })} />
          </Field>
          <Field label={t("add.shortcut")}>
            <input className="field" value={entry.shortcut ?? ""} placeholder="App.exe" onChange={(e) => onChange({ ...entry, shortcut: e.target.value || null })} />
          </Field>
        </>
      )}
      {!isWinget && entry.mode === "silent" && (
        <details open={!compact} className="rounded-lg bg-[var(--fill)] px-3 py-2">
          <summary className="cursor-pointer text-[13px] font-medium">{t("add.advanced")}</summary>
          <div className="mt-2">
            <Field label={t("add.args")} hint={t("add.argsHint")}>
              <input
                className="field font-mono text-[13px]"
                value={args}
                spellCheck={false}
                placeholder="/S"
                onChange={(e) => setArgs(e.target.value)}
                onBlur={() => onChange({ ...entry, silent_args: splitArgs(args) })}
              />
            </Field>
          </div>
        </details>
      )}
      <div className="flex items-center justify-between">
        <span className="text-[13px]">{t("add.include")}</span>
        <Toggle checked={entry.include_in_install_all} onChange={(v) => onChange({ ...entry, include_in_install_all: v })} label={t("add.include")} />
      </div>
    </div>
  );
}

function Meta({ entry }: { entry: MyApp }) {
  const { t } = useT();
  const type = entry.installer_type ?? entry.file_type ?? (entry.source.type === "winget" ? entry.source.store : "—");
  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-[12px]">
      <dt className="text-[var(--secondary)]">{t("add.publisher")}</dt>
      <dd className="truncate">{entry.publisher ?? "—"}</dd>
      <dt className="text-[var(--secondary)]">{t("add.size")}</dt>
      <dd>{entry.size ? fmtSize(entry.size) : t("add.unknownSize")}</dd>
      <dt className="text-[var(--secondary)]">{t("add.type")}</dt>
      <dd>{type}{entry.version ? ` · ${entry.version}` : ""}</dd>
      <dt className="text-[var(--secondary)]">{t("add.source")}</dt>
      <dd className="selectable truncate font-mono text-[11px]" title={sourceLabel(entry.source)}>{sourceLabel(entry.source)}</dd>
    </dl>
  );
}

// ---------- Add / Change link ----------

type Row = { a: Analysis; entry?: MyApp; added?: boolean; info?: string };

export function AddSheet({ replace, onClose }: { replace?: MyApp; onClose: () => void }) {
  const { t } = useT();
  const { settings, go } = useApp();
  const [text, setText] = useState("");
  const [rows, setRows] = useState<Row[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const analyze = async (input: string) => {
    setBusy(true);
    setError(null);
    try {
      const res = await api.myappsAnalyze(input);
      setRows(res.map((a) => ({ a, entry: a.kind === "ready" ? a.entry : undefined })));
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };

  // Re-analyze one row in place (picked winget match or page link).
  const resolveRow = async (i: number, input: string) => {
    const [a] = await api.myappsAnalyze(input);
    setRows((r) => r!.map((x, j) => (j === i ? { a, entry: a.kind === "ready" ? a.entry : undefined } : x)));
  };

  const add = async (idx: number[]) => {
    setError(null);
    try {
      await api.myappsAdd(idx.map((i) => rows![i].entry!));
      setRows((r) => r!.map((x, j) => (idx.includes(j) ? { ...x, added: true } : x)));
    } catch (e) {
      setError(String(e));
    }
  };

  const replaceWith = async (e: MyApp) => {
    try {
      await api.myappsUpdate({ ...replace!, source: e.source, size: e.size, file_type: e.file_type, version: e.version, silent_args: e.file_type === replace!.file_type ? replace!.silent_args : e.silent_args });
      onClose();
    } catch (err) {
      setError(String(err));
    }
  };

  const drive = async (i: number, a: Extract<Analysis, { kind: "drive" }>) => {
    const files = a.file ? [{ id: a.file, name: a.name ?? a.file, path: "" }] : (await api.driveList(a.folder!)).filter((f) => !f.is_folder).map(({ id, name, path }) => ({ id, name, path }));
    await api.driveDownload(files, settings.drive_dest);
    setRows((r) => r!.map((x, j) => (j === i ? { ...x, info: t("add.driveStarted") } : x)));
  };

  const ready = rows?.map((r, i) => (r.entry && !r.added ? i : -1)).filter((i) => i >= 0) ?? [];

  return (
    <Sheet
      title={replace ? t("add.replaceTitle", { name: replace.name }) : t("add.title")}
      onClose={onClose}
      width={640}
      footer={
        rows ? (
          <>
            <Button variant="plain" onClick={() => setRows(null)}>{t("add.back")}</Button>
            {!replace && ready.length > 1 && <Button variant="primary" onClick={() => add(ready)}>{t("add.addAll", { n: ready.length })}</Button>}
            <Button onClick={onClose}>{t("add.done")}</Button>
          </>
        ) : (
          <>
            <Button variant="plain" onClick={onClose}>{t("common.cancel")}</Button>
            <Button variant="primary" disabled={!text.trim() || busy} onClick={() => analyze(text)}>
              {busy ? <ProgressRing /> : t("add.analyze")}
            </Button>
          </>
        )
      }
    >
      {error && <p className="selectable mb-3 rounded-lg bg-[var(--fill)] p-2 text-[13px] text-[var(--red)]">{error}</p>}
      {!rows ? (
        <div className="flex flex-col gap-2">
          <div className="relative">
            <textarea
              className="field min-h-[132px] resize-y py-3 pr-28 font-mono text-[14px]"
              placeholder={t("add.placeholder")}
              aria-label={t("add.placeholder")}
              value={text}
              spellCheck={false}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && e.ctrlKey && text.trim() && analyze(text)}
            />
            <Button
              className="absolute right-2 top-2"
              onClick={async () => {
                // Clipboard is read only on this explicit press — never in the background.
                const c = (await readText().catch(() => "")) ?? "";
                if (c) setText((x) => (x.trim() ? `${x.trimEnd()}\n${c}` : c));
              }}
            >
              <ClipboardPaste size={14} /> {t("add.paste")}
            </Button>
          </div>
          <p className="text-[12px] text-[var(--secondary)]">{t("add.multiHint")}</p>
          {busy && (
            <p className="flex items-center gap-2 text-[13px] text-[var(--secondary)]">
              <ProgressRing /> {t("add.analyzing")}
            </p>
          )}
        </div>
      ) : (
        <ul className="flex flex-col gap-3">
          {rows.map((r, i) => (
            <li key={i} className="rounded-xl border border-[var(--card-border)] bg-[var(--card)] p-3">
              <div className="mb-2 truncate font-mono text-[11px] text-[var(--tertiary)]" title={r.a.input}>{r.a.input}</div>
              {r.entry && (
                <div className="flex items-start gap-3">
                  <AppIcon id={r.entry.id || r.entry.name} name={r.entry.name} src={r.entry.icon} size={48} />
                  <div className="min-w-0 flex-1">
                    <Meta entry={r.entry} />
                    <div className="mt-3">
                      <EntryFields entry={r.entry} compact onChange={(e) => setRows((x) => x!.map((y, j) => (j === i ? { ...y, entry: e } : y)))} />
                    </div>
                    <div className="mt-3 flex justify-end gap-2">
                      {r.added ? (
                        <Badge tone="green">{t("add.added")}</Badge>
                      ) : replace ? (
                        <Button variant="primary" onClick={() => replaceWith(r.entry!)}>{t("add.useLink")}</Button>
                      ) : (
                        <>
                          <Button variant="plain" onClick={() => setRows((x) => x!.filter((_, j) => j !== i))}>{t("common.cancel")}</Button>
                          <Button variant="primary" disabled={!r.entry.name.trim()} onClick={() => add([i])}>{t("add.add")}</Button>
                        </>
                      )}
                    </div>
                  </div>
                </div>
              )}
              {r.a.kind === "winget_choices" && (
                <div>
                  <div className="mb-1 text-[13px] font-semibold">{t("add.pick")}</div>
                  <ul className="divide-y divide-[var(--separator)] rounded-lg bg-[var(--fill)]">
                    {r.a.matches.map((m) => (
                      <li key={m.id} className="flex items-center gap-3 px-3 py-2 text-[13px]">
                        <div className="min-w-0 flex-1">
                          <div className="font-medium">{m.name}</div>
                          <div className="font-mono text-[11px] text-[var(--secondary)]">{m.id} · {m.version}</div>
                        </div>
                        <Button onClick={() => resolveRow(i, m.id)}>{t("add.use")}</Button>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              {r.a.kind === "page_links" && (
                <div>
                  <div className="mb-1 text-[13px] font-semibold">{t("add.pickLink")}</div>
                  <ul className="divide-y divide-[var(--separator)] rounded-lg bg-[var(--fill)]">
                    {r.a.links.map((l) => (
                      <li key={l} className="flex items-center gap-3 px-3 py-2">
                        <span className="selectable min-w-0 flex-1 truncate font-mono text-[11px]" title={l}>{l}</span>
                        <Button onClick={() => resolveRow(i, l)}>{t("add.use")} <ChevronRight size={13} /></Button>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              {r.a.kind === "drive" && (
                <div className="flex items-center justify-between gap-3">
                  <span className="text-[13px]">{r.info ?? t("add.drive")}{r.a.name ? ` (${r.a.name})` : ""}</span>
                  {r.info ? (
                    <Button variant="plain" onClick={() => (onClose(), go("files"))}>{t("nav.files")}</Button>
                  ) : (
                    <Button variant="primary" onClick={() => drive(i, r.a as Extract<Analysis, { kind: "drive" }>)}>{t("add.driveDo")}</Button>
                  )}
                </div>
              )}
              {r.a.kind === "error" && (
                <div className="flex items-start justify-between gap-3">
                  <div className="flex gap-2">
                    <AlertTriangle size={16} className="mt-0.5 shrink-0 text-[var(--red)]" />
                    <div>
                      <div className="text-[13px] text-[var(--red)]">{r.a.message}</div>
                      {r.a.hint && <div className="text-[12px] text-[var(--secondary)]">{r.a.hint}</div>}
                    </div>
                  </div>
                  {r.a.open_url && <Button onClick={() => openUrl(r.a.kind === "error" ? r.a.open_url! : "")}>{t("add.openPage")}</Button>}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </Sheet>
  );
}

// ---------- Edit / Review ----------

export function EntrySheet({ entry: initial, review, onClose }: { entry: MyApp; review: boolean; onClose: () => void }) {
  const { t } = useT();
  const { openAdd } = useApp();
  const [e, setE] = useState(initial);
  const [confirmUnsigned, setConfirmUnsigned] = useState(initial.allow_unsigned);
  const [error, setError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [log, setLog] = useState<string | null>(null);
  const job = useJob(e.id);

  const sig = e.signature;
  const unsigned = sig === "NotSigned";
  const invalid = !!sig && !["Valid", "NotSigned", "Archive"].includes(sig);
  const blocked = review && (invalid || (unsigned && !confirmUnsigned));

  const save = async (patch: Partial<MyApp> = {}) => {
    setError(null);
    try {
      await api.myappsUpdate({ ...e, ...patch });
      return true;
    } catch (err) {
      setError(String(err));
      return false;
    }
  };

  const approve = async (test: boolean) => {
    if (!(await save({ reviewed: true, allow_unsigned: unsigned && confirmUnsigned }))) return;
    setLog(null);
    setTesting(test);
    await api.install([e.id]);
    if (!test) onClose();
  };

  useEffect(() => {
    if (!testing || !job) return;
    if (job.phase === "done") {
      setTesting(false);
      setLog(job.reboot ? t("review.exitReboot") : t("review.exit", { c: 0 }));
    }
    if (job.phase === "failed") {
      setTesting(false);
      api.logTail(e.id).then((l) => setLog(`${job.message ?? ""}\n\n${l}`));
    }
  }, [job?.phase, testing]);

  const typeLabel = e.installer_type === "unknown" ? t("review.unknownType") : e.installer_type ?? e.file_type ?? "—";

  return (
    <Sheet
      title={review ? t("review.title", { name: initial.name }) : t("edit.title", { name: initial.name })}
      onClose={onClose}
      footer={
        review ? (
          <>
            <Button variant="plain" onClick={onClose}>{t("common.cancel")}</Button>
            <Button disabled={blocked || testing} onClick={() => approve(true)}>{testing ? <><ProgressRing /> {t("review.testing")}</> : t("review.test")}</Button>
            <Button variant="primary" disabled={blocked || testing} onClick={() => approve(false)}>{t("review.install")}</Button>
          </>
        ) : (
          <>
            <Button variant="plain" onClick={() => (onClose(), openAdd(initial))}>{t("my.menu.changeLink")}</Button>
            <Button variant="plain" onClick={onClose}>{t("common.cancel")}</Button>
            <Button variant="primary" onClick={async () => (await save()) && onClose()}>{t("review.save")}</Button>
          </>
        )
      }
    >
      <div className="mb-4 flex gap-3">
        <AppIcon id={e.id} name={e.name} src={e.icon} size={48} />
        <div className="min-w-0 flex-1">
          {review && <p className="mb-2 text-[13px] text-[var(--secondary)]">{t("review.body")}</p>}
          <Meta entry={e} />
        </div>
      </div>

      {review && (
        <div className="mb-4 flex flex-col gap-2 rounded-xl bg-[var(--fill)] p-3 text-[13px]">
          {sig === "Valid" && (
            <div className="flex items-center gap-2 text-[var(--green)]"><ShieldCheck size={17} /> {t("review.signed", { p: e.signer ?? "?" })}</div>
          )}
          {sig === "Archive" && (
            <div className="flex items-center gap-2 text-[var(--secondary)]"><Archive size={17} /> {t("review.archive")}</div>
          )}
          {unsigned && (
            <>
              <div className="flex items-center gap-2 font-semibold text-[var(--orange)]"><ShieldAlert size={17} /> {t("review.unsigned")}</div>
              <label className="flex cursor-pointer items-center gap-2">
                <input type="checkbox" className="h-4 w-4 accent-[var(--accent)]" checked={confirmUnsigned} onChange={(x) => setConfirmUnsigned(x.target.checked)} />
                {t("review.unsignedConfirm")}
              </label>
            </>
          )}
          {invalid && <div className="flex items-center gap-2 text-[var(--red)]"><ShieldAlert size={17} /> {t("review.invalid", { s: sig! })}</div>}
          <div className="text-[var(--secondary)]">{t("review.type")}: <span className="font-medium text-[var(--label)]">{typeLabel}</span></div>
        </div>
      )}

      <EntryFields entry={e} onChange={setE} />
      {error && <p className="selectable mt-3 text-[13px] text-[var(--red)]">{error}</p>}
      {log && (
        <div className="mt-3">
          <div className="caption mb-1">{t("review.log")}</div>
          <pre className="selectable max-h-48 overflow-auto whitespace-pre-wrap rounded-lg bg-[var(--fill)] p-3 text-[11px]">{log}</pre>
        </div>
      )}
    </Sheet>
  );
}

// ---------- Import review ----------

export function ImportSheet({ entries, fromUrl, onClose }: { entries: MyApp[]; fromUrl?: boolean; onClose: () => void }) {
  const { t } = useT();
  const [sel, setSel] = useState<Set<string>>(new Set(entries.map((e) => e.id)));
  const [error, setError] = useState<string | null>(null);
  const modeLabel = (m: Mode) => t(`my.mode.${m}` as Key);
  return (
    <Sheet
      title={t("import.title")}
      onClose={onClose}
      width={720}
      footer={
        <>
          <Button variant="plain" onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={sel.size === 0}
            onClick={async () => {
              try {
                await api.myappsAdd(entries.filter((e) => sel.has(e.id)));
                onClose();
              } catch (e) {
                setError(String(e));
              }
            }}
          >
            {t("import.add", { n: sel.size })}
          </Button>
        </>
      }
    >
      {fromUrl && <Badge>{t("import.fromUrl")}</Badge>}
      <p className="my-2 text-[13px] text-[var(--secondary)]">{entries.length ? t("import.body") : t("import.none")}</p>
      {error && <p className="selectable mb-2 text-[13px] text-[var(--red)]">{error}</p>}
      <ul className="divide-y divide-[var(--separator)] rounded-xl border border-[var(--card-border)] bg-[var(--card)]">
        {entries.map((e) => (
          <li key={e.id} className="flex items-start gap-3 px-3 py-2.5">
            <input
              type="checkbox"
              className="mt-1 h-4 w-4 accent-[var(--accent)]"
              aria-label={e.name}
              checked={sel.has(e.id)}
              onChange={(x) => setSel((s) => { const n = new Set(s); x.target.checked ? n.add(e.id) : n.delete(e.id); return n; })}
            />
            <div className="min-w-0 flex-1">
              <div className="font-medium">{e.name}</div>
              <div className="selectable break-all font-mono text-[11px] text-[var(--secondary)]">{sourceLabel(e.source)}</div>
              <div className="mt-0.5 text-[12px] text-[var(--secondary)]">
                {modeLabel(e.mode)}
                {e.mode === "silent" && e.silent_args.length > 0 && <> · <code className="selectable font-mono">{e.silent_args.join(" ")}</code></>}
                {e.mode === "extract" && <> · {e.extract_to ?? ""}</>}
                {!e.include_in_install_all && <> · {t("my.notInAll")}</>}
              </div>
            </div>
          </li>
        ))}
      </ul>
    </Sheet>
  );
}
