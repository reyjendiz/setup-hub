import { CircleCheck, CircleDashed } from "lucide-react";
import { useEffect, useState } from "react";
import { AdState, api, useJob } from "../api";
import { AppIcon, Button, Card, JobButton, PageHeader } from "../components";
import { useT } from "../i18n";

export default function AD() {
  const { t } = useT();
  const [st, setSt] = useState<AdState | null>(null);
  const [chain, setChain] = useState(false);
  const job = useJob("ad");
  const discordJob = useJob("discord");

  useEffect(() => {
    api.adState().then(setSt);
  }, [job?.phase, discordJob?.phase]);

  // "Install Discord, then AD": start AD once Discord's install finishes.
  useEffect(() => {
    if (chain && discordJob?.phase === "done") {
      setChain(false);
      api.adInstall();
    }
    if (chain && discordJob?.phase === "failed") setChain(false);
  }, [chain, discordJob?.phase]);

  const rows: { label: string; ok?: boolean; auto?: boolean }[] = [
    { label: t("ad.discord"), ok: st?.discord },
    { label: t("ad.dotnet"), ok: st?.dotnet, auto: true },
    { label: t("ad.vencord"), ok: st?.vencord_cli, auto: true },
  ];

  return (
    <>
      <PageHeader title={t("ad.title")} subtitle={t("ad.subtitle")} />
      <Card lift={false} className="max-w-2xl p-5">
        <div className="flex items-start gap-4">
          <AppIcon id="ad" name="AD" size={56} />
          <div className="min-w-0 flex-1">
            <div className="section-title">AD</div>
            <p className="text-[13px] text-[var(--secondary)]">{t("ad.cardDesc")}</p>
          </div>
          <JobButton
            job={job}
            done={!!st?.installed}
            idleLabel={t("ad.install")}
            redoLabel={t("ad.reinstall")}
            disabled={!st || !st.discord || chain}
            onStart={() => api.adInstall()}
          />
        </div>

        <ul className="mt-5 divide-y divide-[var(--separator)] rounded-xl bg-[var(--fill)]">
          {rows.map((r) => (
            <li key={r.label} className="flex items-center justify-between px-4 py-2.5 text-[14px]">
              <span className="flex items-center gap-2">
                {r.ok ? <CircleCheck size={17} className="text-[var(--green)]" /> : <CircleDashed size={17} className="text-[var(--tertiary)]" />}
                {r.label}
              </span>
              <span className="text-[13px] text-[var(--secondary)]">{r.ok ? t("ad.ready") : r.auto ? t("ad.willInstall") : t("ad.missing")}</span>
            </li>
          ))}
        </ul>

        {st && !st.discord && (
          <div className="mt-4 flex items-center justify-between gap-4 rounded-xl border border-[var(--card-border)] px-4 py-3">
            <span className="text-[14px]">{t("ad.noDiscord")}</span>
            <Button
              variant="primary"
              disabled={chain}
              onClick={() => {
                setChain(true);
                api.install(["discord"]);
              }}
            >
              {t("ad.installBoth")}
            </Button>
          </div>
        )}
        {job?.phase === "done" && <p className="mt-4 text-[13px] text-[var(--green)]">{t("ad.done")}</p>}
      </Card>
    </>
  );
}
