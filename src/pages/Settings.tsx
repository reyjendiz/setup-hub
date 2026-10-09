import { open, save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { ReactNode, useState } from "react";
import { api } from "../api";
import { useApp } from "../App";
import { Button, Card, PageHeader, Toggle } from "../components";
import { useT } from "../i18n";

function Section({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return (
    <section className="mb-6">
      <h2 className="caption mb-2 px-1">{title}</h2>
      <Card lift={false} className="divide-y divide-[var(--separator)]">
        {children}
      </Card>
      {note && <p className="mt-1.5 px-1 text-[12px] text-[var(--secondary)]">{note}</p>}
    </section>
  );
}

function Line({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex min-h-[52px] items-center justify-between gap-4 px-4 py-2.5">
      <div className="min-w-0">
        <div className="text-[14px]">{label}</div>
        {hint && <div className="text-[12px] text-[var(--secondary)]">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: [T, string][]; onChange: (v: T) => void; label: string }) {
  return (
    <div role="radiogroup" aria-label={label} className="flex rounded-lg bg-[var(--fill)] p-0.5 text-[13px] font-medium">
      {options.map(([v, l]) => (
        <button
          key={v}
          role="radio"
          aria-checked={value === v}
          onClick={() => onChange(v)}
          className={`cursor-pointer rounded-md px-3 py-1 ${value === v ? "bg-[var(--card)] shadow-sm" : "text-[var(--secondary)]"}`}
        >
          {l}
        </button>
      ))}
    </div>
  );
}

function SecretField({ name, initiallySet, label, hint }: { name: "github_token" | "google_api_key"; initiallySet: boolean; label: string; hint: string }) {
  const { t } = useT();
  const [isSet, setIsSet] = useState(initiallySet);
  const [v, setV] = useState("");
  const save = async (value: string) => {
    await api.setSecret(name, value);
    setIsSet(!!value.trim());
    setV("");
  };
  return (
    <Line label={label} hint={hint}>
      <input
        type="password"
        className="field w-56"
        placeholder={isSet ? "••••••••  " + t("settings.set") : t("settings.notSet")}
        aria-label={label}
        value={v}
        autoComplete="off"
        onChange={(e) => setV(e.target.value)}
      />
      {v ? <Button variant="primary" onClick={() => save(v)}>{t("common.save")}</Button> : isSet && <Button variant="danger" onClick={() => save("")}>{t("settings.clear")}</Button>}
    </Line>
  );
}

export default function SettingsPage() {
  const { t } = useT();
  const { settings: s, updateSettings, boot, setItems, reviewImport } = useApp();
  const [catalogMsg, setCatalogMsg] = useState<string | null>(null);
  const [logMsg, setLogMsg] = useState<string | null>(null);
  const [myMsg, setMyMsg] = useState<string | null>(null);
  const [url, setUrl] = useState(s.catalog_url);
  const [listUrl, setListUrl] = useState(s.my_apps_url);
  const fail = (e: unknown) => setMyMsg(String(e));

  return (
    <div className="max-w-2xl">
      <PageHeader title={t("settings.title")} />

      <Section title={t("settings.theme")}>
        <Line label={t("settings.language")}>
          <Segmented label={t("settings.language")} value={s.lang} options={[["en", "English"], ["ru", "Русский"]]} onChange={(lang) => updateSettings({ lang })} />
        </Line>
        <Line label={t("settings.theme")}>
          <Segmented
            label={t("settings.theme")}
            value={s.theme}
            options={[["system", t("settings.theme.system")], ["light", t("settings.theme.light")], ["dark", t("settings.theme.dark")]]}
            onChange={(theme) => updateSettings({ theme })}
          />
        </Line>
      </Section>

      <Section title={t("settings.downloads")}>
        <Line label={t("settings.driveDest")} hint={s.drive_dest}>
          <Button
            onClick={async () => {
              const d = await open({ directory: true, defaultPath: s.drive_dest });
              if (typeof d === "string") updateSettings({ drive_dest: d });
            }}
          >
            {t("files.change")}
          </Button>
        </Line>
        <Line label={t("settings.keep")}>
          <Toggle checked={s.keep_installers} onChange={(keep_installers) => updateSettings({ keep_installers })} label={t("settings.keep")} />
        </Line>
        <Line label={t("settings.parallel")}>
          <Segmented
            label={t("settings.parallel")}
            value={String(s.parallel_downloads)}
            options={["1", "2", "3", "4"].map((n) => [n, n])}
            onChange={(n) => updateSettings({ parallel_downloads: Number(n) })}
          />
        </Line>
      </Section>

      <Section title={t("settings.secrets")} note={t("settings.secretsNote")}>
        <SecretField name="github_token" initiallySet={boot.has_github_token} label={t("settings.github")} hint={t("settings.githubHint")} />
        <SecretField name="google_api_key" initiallySet={boot.has_google_key} label={t("settings.google")} hint={t("settings.googleHint")} />
      </Section>

      <Section title={t("settings.myapps")} note={myMsg ?? undefined}>
        <Line label={t("settings.export")}>
          <Button
            onClick={async () => {
              const p = await save({ defaultPath: "my_apps.json", filters: [{ name: "JSON", extensions: ["json"] }] });
              if (p) api.myappsExport(p).then(() => setMyMsg(t("settings.myExported", { p })), fail);
            }}
          >
            {t("settings.export")}
          </Button>
        </Line>
        <Line label={t("settings.import")}>
          <Button
            onClick={async () => {
              const p = await open({ filters: [{ name: "JSON", extensions: ["json"] }] });
              if (typeof p === "string") api.myappsImport(p).then((e) => reviewImport(e), fail);
            }}
          >
            {t("settings.import")}
          </Button>
        </Line>
        <div className="flex flex-col gap-2 px-4 py-3">
          <label className="text-[14px]" htmlFor="listurl">{t("settings.listUrl")}</label>
          <div className="flex gap-2">
            <input
              id="listurl"
              className="field"
              placeholder="https://drive.google.com/file/d/…/view"
              value={listUrl}
              onChange={(e) => setListUrl(e.target.value)}
              onBlur={() => updateSettings({ my_apps_url: listUrl.trim() })}
            />
            <Button
              disabled={!listUrl.trim()}
              onClick={() => (updateSettings({ my_apps_url: listUrl.trim() }), api.myappsFetchUrl(listUrl.trim()).then((e) => reviewImport(e, true), fail))}
            >
              {t("settings.loadNow")}
            </Button>
          </div>
          <p className="text-[12px] text-[var(--secondary)]">{t("settings.listUrlHint")}</p>
        </div>
        <Line label={t("settings.allowHttp")} hint={t("settings.allowHttpHint")}>
          <Toggle checked={s.allow_http} onChange={(allow_http) => updateSettings({ allow_http })} label={t("settings.allowHttp")} />
        </Line>
      </Section>

      <Section title={t("settings.catalog")} note={catalogMsg ?? undefined}>
        <div className="flex flex-col gap-2 px-4 py-3">
          <label className="text-[14px]" htmlFor="caturl">{t("settings.catalogUrl")}</label>
          <input
            id="caturl"
            className="field"
            placeholder="https://raw.githubusercontent.com/<you>/<repo>/main/catalog.json"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onBlur={() => updateSettings({ catalog_url: url.trim() })}
          />
        </div>
        <Line label={t("settings.checkCatalog")}>
          <Button
            onClick={async () => {
              updateSettings({ catalog_url: url.trim() });
              const [items, origin] = await api.reloadCatalog(url);
              setItems(items);
              setCatalogMsg(t("settings.catalogLoaded", { n: items.length, origin }));
            }}
          >
            {t("common.refresh")}
          </Button>
        </Line>
      </Section>

      <Section title={t("settings.logs")} note={logMsg ?? undefined}>
        <Line label={t("settings.exportLog")}>
          <Button variant="plain" onClick={() => api.logsDir().then((d) => revealItemInDir(d))}>{t("apps.openLogs")}</Button>
          <Button
            onClick={() =>
              api.exportLog().then(
                (p) => (setLogMsg(t("settings.exported", { p })), revealItemInDir(p)),
                (e) => setLogMsg(String(e)),
              )
            }
          >
            {t("settings.exportLog")}
          </Button>
        </Line>
      </Section>

      <Section title={t("settings.about")}>
        <div className="px-4 py-3 text-[13px] text-[var(--secondary)]">
          <p>{t("settings.aboutBody", { v: boot.version })}</p>
          <p className="mt-1">{t("settings.licenses")}</p>
        </div>
      </Section>
    </div>
  );
}
