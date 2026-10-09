import { openUrl } from "@tauri-apps/plugin-opener";
import { ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { api, License } from "../api";
import { Badge, Button, Card, PageHeader, ProgressRing } from "../components";
import { Key, useT } from "../i18n";

export default function Activation() {
  const { t } = useT();
  const [lic, setLic] = useState<License[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [out, setOut] = useState<string | null>(null);

  const load = () => api.licenseStatus().then(setLic, (e) => setErr(String(e)));
  useEffect(() => void load(), []);

  const submit = async () => {
    setBusy(true);
    setOut(null);
    try {
      setOut(await api.activateKey(key));
      setKey("");
    } catch (e) {
      setOut(String(e));
    }
    setBusy(false);
    load();
  };
  const valid = /^[A-Z0-9]{5}(-[A-Z0-9]{5}){4}$/.test(key);

  return (
    <>
      <PageHeader title={t("act.title")} subtitle={t("act.subtitle")}>
        <Button variant="primary" className="h-10 px-5 text-[15px]" onClick={() => openUrl("ms-settings:activation")}>
          {t("act.openSettings")}
        </Button>
      </PageHeader>

      <div className="flex max-w-2xl flex-col gap-4">
        <Card lift={false} className="p-5">
          {err && <p className="text-[var(--red)]">{err}</p>}
          {!lic && !err && <ProgressRing />}
          {lic?.length === 0 && <p className="text-[var(--secondary)]">{t("act.none")}</p>}
          {lic?.map((l) => (
            <div key={l.partial_key} className="flex items-start gap-4">
              <ShieldCheck size={28} className={l.status === 1 ? "text-[var(--green)]" : "text-[var(--orange)]"} />
              <div className="min-w-0 flex-1">
                <div className="section-title">{l.name.replace(/^Windows\(R\),\s*/, "Windows ")}</div>
                <div className="mt-1 flex flex-wrap items-center gap-2 text-[13px] text-[var(--secondary)]">
                  <Badge tone={l.status === 1 ? "green" : "orange"}>{t(`act.s${l.status}` as Key)}</Badge>
                  <span>{l.description}</span>
                </div>
                <div className="mt-2 text-[13px] text-[var(--secondary)]">
                  {t("act.partialKey")} <span className="font-mono">{l.partial_key}</span>
                </div>
                {l.status !== 1 && l.grace_minutes > 0 && <div className="text-[13px] text-[var(--orange)]">{t("act.grace", { d: Math.floor(l.grace_minutes / 1440) })}</div>}
              </div>
            </div>
          ))}
        </Card>

        <Card lift={false} className="p-5">
          <div className="mb-3 font-semibold">{t("act.enterKey")}</div>
          <form
            className="flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (valid && !busy) submit();
            }}
          >
            <input
              className="field font-mono tracking-wider"
              placeholder={t("act.keyPlaceholder")}
              aria-label={t("act.enterKey")}
              value={key}
              maxLength={29}
              autoComplete="off"
              spellCheck={false}
              onChange={(e) => setKey(e.target.value.toUpperCase().replace(/[^A-Z0-9-]/g, ""))}
            />
            <Button variant="primary" type="submit" className="h-9" disabled={!valid || busy}>
              {busy ? <ProgressRing /> : t("act.submit")}
            </Button>
          </form>
          <p className="mt-2 text-[12px] text-[var(--secondary)]">{t("act.keyNote")}</p>
          {out && <pre className="mt-3 max-h-48 overflow-auto whitespace-pre-wrap rounded-lg bg-[var(--fill)] p-3 text-[12px]">{out}</pre>}
        </Card>

        <Card lift={false} className="p-5">
          <div className="mb-1 font-semibold">{t("act.msaTitle")}</div>
          <p className="text-[13px] text-[var(--secondary)]">{t("act.msaBody")}</p>
        </Card>
      </div>
    </>
  );
}
