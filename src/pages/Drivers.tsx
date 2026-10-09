import { openUrl } from "@tauri-apps/plugin-opener";
import { Cpu, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { api, Gpu, GpuInfo, useJob } from "../api";
import { useApp } from "../App";
import { Badge, Button, Card, JobButton, PageHeader, ProgressRing, Toggle } from "../components";
import { useT } from "../i18n";

export default function Drivers() {
  const { t } = useT();
  const [info, setInfo] = useState<GpuInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const job = useJob("nvidia");

  useEffect(() => {
    api.gpuInfo().then(setInfo, (e) => setError(String(e)));
  }, [job?.phase === "done"]);

  const nv = info?.gpus.find((g) => g.vendor === "Nvidia");

  return (
    <>
      <PageHeader title={t("drivers.title")} subtitle={t("drivers.subtitle")} />
      {error && <p className="text-[var(--red)]">{error}</p>}
      {!info && !error && (
        <p className="flex items-center gap-2 text-[var(--secondary)]">
          <ProgressRing /> {t("drivers.detecting")}
        </p>
      )}
      {info && info.gpus.length === 0 && !info.detect_error && <p className="text-[var(--secondary)]">{t("drivers.none")}</p>}

      <div className="flex max-w-3xl flex-col gap-4">
        {/* Windows couldn't list the cards: the NVIDIA App finds the card itself. */}
        {info?.detect_error && (
          <GpuCard title={t("drivers.unknownTitle")} subtitle={t("drivers.detectFailed", { e: info.detect_error })}>
            <NvidiaInstall info={info} />
          </GpuCard>
        )}
        {info?.gpus.map((g, i) => (
          <GpuCard
            key={`${g.name}-${i}`}
            title={g.driver_missing ? t(g.vendor === "Nvidia" ? "drivers.nvidiaNoDriver" : "drivers.noDriver") : g.name}
            subtitle={`${t("drivers.installed")}: ${g.driver_missing ? t("drivers.basicDisplay") : g.display_version ?? g.driver_version}`}
          >
            {g === nv && <NvidiaInstall info={info} gpu={g} />}
            {g.vendor === "Amd" && (
              <div className="mt-4 flex items-center justify-between gap-4">
                <p className="text-[13px] text-[var(--secondary)]">{t("drivers.amd")}</p>
                <Button onClick={() => openUrl("https://www.amd.com/en/support/download/drivers.html")}>{t("drivers.openAmd")}</Button>
              </div>
            )}
            {g.vendor === "Intel" && (
              <div className="mt-4 flex items-center justify-between gap-4">
                <p className="text-[13px] text-[var(--secondary)]">{t("drivers.intel")}</p>
                <Button onClick={() => openUrl("https://www.intel.com/content/www/us/en/support/intel-driver-support-assistant.html")}>{t("drivers.openIntel")}</Button>
              </div>
            )}
          </GpuCard>
        ))}
      </div>
    </>
  );
}

function GpuCard({ title, subtitle, children }: { title: string; subtitle: string; children?: React.ReactNode }) {
  return (
    <Card lift={false} className="p-5">
      <div className="flex items-start gap-4">
        <div className="grid h-12 w-12 shrink-0 place-items-center rounded-xl bg-[var(--fill)] text-[var(--link)]">
          <Cpu size={24} strokeWidth={1.7} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="section-title">{title}</div>
          <div className="selectable mt-1 text-[13px] text-[var(--secondary)]">{subtitle}</div>
          {children}
        </div>
      </div>
    </Card>
  );
}

/** Driver straight from NVIDIA when Windows knows the model; otherwise the NVIDIA App, which detects the card itself. */
function NvidiaInstall({ info, gpu }: { info: GpuInfo; gpu?: Gpu }) {
  const { t } = useT();
  const { settings, updateSettings } = useApp();
  const job = useJob("nvidia");
  const latest = info.nvidia_latest;
  const app = info.use_nvidia_app;
  const reason = gpu?.driver_missing ? t("drivers.noModel") : info.nvidia_error ? t("drivers.lookupFailed", { e: info.nvidia_error }) : null;

  return (
    <div className="mt-4 flex flex-col gap-3">
      {latest && (
        <div className="flex flex-wrap items-center gap-3">
          <span className="text-[14px]">
            {t("drivers.latest")}: <b>{latest.version}</b>
          </span>
          {info.update_available ? <Badge tone="orange">{t("drivers.updateAvailable")}</Badge> : <Badge tone="green">{t("drivers.upToDate", { v: latest.version })}</Badge>}
          <span className="text-[12px] text-[var(--tertiary)]">{t("drivers.released", { d: latest.release_date, s: latest.size })}</span>
        </div>
      )}
      {app && reason && <p className="text-[13px] text-[var(--secondary)]">{reason}</p>}
      {job?.phase === "done" && job.message === "nvidia-app-fallback" && <p className="text-[13px] text-[var(--green)]">{t("drivers.appFallback")}</p>}
      {!app && (
        <div className="flex items-center justify-between gap-4 rounded-xl bg-[var(--fill)] px-3 py-2.5">
          <div>
            <div className="text-[14px] font-medium">{t("drivers.clean")}</div>
            <div className="text-[12px] text-[var(--secondary)]">{t("drivers.cleanHint")}</div>
          </div>
          <Toggle checked={settings.clean_driver_install} onChange={(v) => updateSettings({ clean_driver_install: v })} label={t("drivers.clean")} />
        </div>
      )}
      <div className="flex items-center justify-between gap-4">
        <p className="flex items-center gap-1.5 text-[12px] text-[var(--secondary)]">
          {!app && (
            <>
              <TriangleAlert size={14} /> {t("drivers.flash")}
            </>
          )}
        </p>
        <JobButton
          job={job}
          done={app ? job?.phase === "done" : !!latest && !info.update_available}
          redoLabel={app ? t("drivers.installApp") : t("drivers.reinstall", { v: latest?.version ?? "" })}
          idleLabel={app ? t("drivers.installApp") : latest ? t("drivers.install", { v: latest.version }) : t("btn.install")}
          onStart={() => api.installNvidia(settings.clean_driver_install)}
          onCancel={() => api.cancel("nvidia")}
        />
      </div>
    </div>
  );
}
