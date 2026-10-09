import { CircleCheck, CircleDashed, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { api, useJob, VenState } from "../api";
import { AppIcon, Button, Card, JobButton, PageHeader, ProgressRing } from "../components";
import { useT } from "../i18n";

export default function Ven() {
  const { t, lang } = useT();
  const [st, setSt] = useState<VenState | null>(null);
  const [chain, setChain] = useState(false);
  const [removing, setRemoving] = useState(false);
  const job = useJob("ven");
  const runJob = useJob("ven-run");
  const discordJob = useJob("discord");

  const load = () => api.venState().then(setSt);
  useEffect(() => void load(), [job?.phase, runJob?.phase, discordJob?.phase]);

  // "Install Discord, then Ven": start Ven's install once Discord's finishes.
  useEffect(() => {
    if (chain && discordJob?.phase === "done") {
      setChain(false);
      api.venInstall();
    }
    if (chain && discordJob?.phase === "failed") setChain(false);
  }, [chain, discordJob?.phase]);

  const rows: { label: string; ok?: boolean }[] = [
    { label: t("ven.discord"), ok: st?.discord },
    { label: t("ven.autostart"), ok: st?.autostart },
    { label: t("ven.vencord"), ok: st?.vencord },
  ];
  const last = st?.last_run;
  const running = runJob && ["queued", "installing"].includes(runJob.phase);
  const failure = job?.phase === "failed" ? job.message : runJob?.phase === "failed" ? runJob.message : null;

  return (
    <>
      <PageHeader title="Ven" subtitle={t("ven.subtitle")} />
      <Card lift={false} className="max-w-2xl p-5">
        <div className="flex items-start gap-4">
          <AppIcon id="ven" name="Ven" size={56} />
          <div className="min-w-0 flex-1">
            <div className="section-title">Ven</div>
            <p className="text-[13px] text-[var(--secondary)]">{t("ven.cardDesc")}</p>
          </div>
          <JobButton
            job={job}
            done={!!st?.installed && st.autostart}
            idleLabel={t("ven.install")}
            redoLabel={t("ven.reinstall")}
            disabled={!st || !st.discord || chain}
            onStart={() => api.venInstall()}
          />
        </div>

        <ul className="mt-5 divide-y divide-[var(--separator)] rounded-xl bg-[var(--fill)]">
          {rows.map((r) => (
            <li key={r.label} className="flex items-center justify-between px-4 py-2.5 text-[14px]">
              <span className="flex items-center gap-2">
                {r.ok ? <CircleCheck size={17} className="text-[var(--green)]" /> : <CircleDashed size={17} className="text-[var(--tertiary)]" />}
                {r.label}
              </span>
              <span className="text-[13px] text-[var(--secondary)]">{r.ok ? t("ven.yes") : t("ven.no")}</span>
            </li>
          ))}
        </ul>

        {last && (
          <p className={`selectable mt-4 flex items-start gap-1.5 text-[13px] ${last.vencord_ok && last.discord_started ? "text-[var(--secondary)]" : "text-[var(--orange)]"}`}>
            {!(last.vencord_ok && last.discord_started) && <TriangleAlert size={14} className="mt-0.5 shrink-0" />}
            {t("ven.lastRun", { when: new Date(last.time * 1000).toLocaleString(lang), msg: last.message })}
          </p>
        )}
        {failure && <p className="selectable mt-2 text-[13px] text-[var(--red)]">{failure}</p>}

        {st && !st.discord && (
          <div className="mt-4 flex items-center justify-between gap-4 rounded-xl border border-[var(--card-border)] px-4 py-3">
            <span className="text-[14px]">{t("ven.noDiscord")}</span>
            <Button
              variant="primary"
              disabled={chain}
              onClick={() => {
                setChain(true);
                api.install(["discord"]);
              }}
            >
              {t("ven.installBoth")}
            </Button>
          </div>
        )}

        {st?.installed && (
          <div className="mt-4 flex items-center justify-end gap-2">
            <Button
              variant="plain"
              disabled={removing}
              onClick={async () => {
                setRemoving(true);
                await api.venRemove().catch(() => {});
                await load();
                setRemoving(false);
              }}
            >
              {t("ven.remove")}
            </Button>
            <Button disabled={!!running} onClick={() => api.venRun()}>
              {running ? <ProgressRing /> : null} {t("ven.runNow")}
            </Button>
          </div>
        )}
      </Card>
      <p className="mt-3 max-w-2xl text-[12px] text-[var(--tertiary)]">{t("ven.how")}</p>
    </>
  );
}
