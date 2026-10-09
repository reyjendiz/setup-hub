import { openUrl } from "@tauri-apps/plugin-opener";
import { Cpu, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { api, GpuInfo, useJob } from "../api";
import { useApp } from "../App";
import { Badge, Button, Card, JobButton, PageHeader, ProgressRing, Toggle } from "../components";
import { useT } from "../i18n";

export default function Drivers() {
  const { t } = useT();
  const { settings, updateSettings } = useApp();
  const [info, setInfo] = useState<GpuInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const job = useJob("nvidia");

  useEffect(() => {
    api.gpuInfo().then(setInfo, (e) => setError(String(e)));
  }, [job?.phase === "done"]);

  const nv = info?.gpus.find((g) => g.vendor === "Nvidia");
  const latest = info?.nvidia_latest;

  return (
    <>
      <PageHeader title={t("drivers.title")} subtitle={t("drivers.subtitle")} />
      {error && <p className="text-[var(--red)]">{error}</p>}
      {!info && !error && (
        <p className="flex items-center gap-2 text-[var(--secondary)]">
          <ProgressRing /> {t("drivers.detecting")}
        </p>
      )}
      {info && info.gpus.length === 0 && <p className="text-[var(--secondary)]">{t("drivers.none")}</p>}

      <div className="flex max-w-3xl flex-col gap-4">
        {info?.gpus.map((g) => (
          <Card key={g.name} lift={false} className="p-5">
            <div className="flex items-start gap-4">
              <div className="grid h-12 w-12 shrink-0 place-items-center rounded-xl bg-[var(--fill)] text-[var(--link)]">
                <Cpu size={24} strokeWidth={1.7} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="section-title">{g.name}</div>
                <div className="mt-1 text-[13px] text-[var(--secondary)]">
                  {t("drivers.installed")}: {g.display_version ?? g.driver_version}
                </div>

                {g.vendor === "Nvidia" && g === nv && (
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
                    {info.nvidia_error && <p className="text-[13px] text-[var(--orange)]">{t("drivers.lookupFailed", { e: info.nvidia_error })}</p>}
                    {job?.phase === "done" && job.message === "nvidia-app-fallback" && <p className="text-[13px] text-[var(--orange)]">{t("drivers.appFallback")}</p>}
                    <div className="flex items-center justify-between gap-4 rounded-xl bg-[var(--fill)] px-3 py-2.5">
                      <div>
                        <div className="text-[14px] font-medium">{t("drivers.clean")}</div>
                        <div className="text-[12px] text-[var(--secondary)]">{t("drivers.cleanHint")}</div>
                      </div>
                      <Toggle checked={settings.clean_driver_install} onChange={(v) => updateSettings({ clean_driver_install: v })} label={t("drivers.clean")} />
                    </div>
                    <div className="flex items-center justify-between gap-4">
                      <p className="flex items-center gap-1.5 text-[12px] text-[var(--secondary)]">
                        <TriangleAlert size={14} /> {t("drivers.flash")}
                      </p>
                      <JobButton
                        job={job}
                        done={!!latest && !info.update_available}
                        redoLabel={t("drivers.reinstall", { v: latest?.version ?? "" })}
                        idleLabel={latest ? t("drivers.install", { v: latest.version }) : t("btn.install")}
                        onStart={() => api.installNvidia(settings.clean_driver_install)}
                        onCancel={() => api.cancel("nvidia")}
                      />
                    </div>
                  </div>
                )}

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
              </div>
            </div>
          </Card>
        ))}
      </div>
    </>
  );
}
