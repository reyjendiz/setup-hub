import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Gauge, Monitor, Moon, MousePointer2, Power } from "lucide-react";
import { useEffect, useState } from "react";
import { api, DefaultState, TweakState } from "../api";
import { useApp } from "../App";
import { AppIcon, Badge, Button, Card, PageHeader, ProgressRing } from "../components";
import { Key, useT } from "../i18n";

const META: Record<string, { icon: typeof Gauge; optional?: boolean }> = {
  mouse_speed: { icon: MousePointer2 },
  mouse_precision: { icon: Gauge },
  power_plan: { icon: Power },
  hibernate: { icon: Moon, optional: true },
  fast_startup: { icon: Monitor, optional: true },
};

export default function Tweaks() {
  const { t } = useT();
  const [states, setStates] = useState<TweakState[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const load = () => api.tweakStates().then(setStates);
  useEffect(() => void load(), []);

  const run = async (id: string, fn: (id: string) => Promise<void>) => {
    setBusy(id);
    setErrors((e) => ({ ...e, [id]: "" }));
    try {
      await fn(id);
    } catch (e) {
      setErrors((x) => ({ ...x, [id]: String(e) }));
    }
    await load();
    setBusy(null);
  };

  const detail = (s: TweakState) => (s.detail === "on" ? t("tweaks.on") : s.detail === "off" ? t("tweaks.off") : s.detail);

  return (
    <>
      <PageHeader title={t("tweaks.title")} subtitle={t("tweaks.subtitle")} />
      <div className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-4">
        {(states ?? Object.keys(META).map((id) => ({ id }) as TweakState)).map((s) => {
          const m = META[s.id];
          const Icon = m.icon;
          const err = errors[s.id] || s.error;
          return (
            <Card key={s.id} className="flex flex-col gap-3 p-4">
              <div className="flex items-start gap-3">
                <div className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-[var(--fill)] text-[var(--link)]">
                  <Icon size={20} strokeWidth={1.8} />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2 font-semibold">
                    {t(`tweaks.${s.id}.title` as Key)}
                    {m.optional && <Badge>{t("tweaks.optional")}</Badge>}
                  </div>
                  <p className="text-[13px] text-[var(--secondary)]">{t(`tweaks.${s.id}.desc` as Key)}</p>
                </div>
              </div>
              {err && <p className="selectable text-[12px] text-[var(--red)]">{err}</p>}
              <div className="mt-auto flex items-center justify-between">
                <span className="text-[13px] text-[var(--secondary)]">{s.detail != null && t("tweaks.current", { v: detail(s) ?? "" })}</span>
                <div className="flex gap-2">
                  {s.can_revert && (
                    <Button variant="plain" disabled={busy !== null} onClick={() => run(s.id, api.tweakRevert)}>
                      {t("tweaks.revert")}
                    </Button>
                  )}
                  {busy === s.id ? (
                    <Button disabled><ProgressRing /></Button>
                  ) : s.applied ? (
                    <Button variant="success" disabled={busy !== null} onClick={() => run(s.id, api.tweakApply)} title={t("tweaks.apply")}>
                      <Check size={15} strokeWidth={3} /> {t("tweaks.applied")}
                    </Button>
                  ) : (
                    <Button variant="primary" disabled={!states || busy !== null} onClick={() => run(s.id, api.tweakApply)}>
                      {t("tweaks.apply")}
                    </Button>
                  )}
                </div>
              </div>
            </Card>
          );
        })}
      </div>
      <DefaultApps />
    </>
  );
}

/** Catalog apps that replace a Windows function (VLC → file types, Lightshot → Print Screen). */
function DefaultApps() {
  const { t } = useT();
  const { boot, installed } = useApp();
  const [states, setStates] = useState<DefaultState[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const load = () => api.defaultsStates().then(setStates, (e) => setErrors({ load: String(e) }));
  // Re-read after installs: a fresh VLC or Lightshot changes what can be applied.
  useEffect(() => void load(), [installed]);

  const run = async (s: DefaultState, revert: boolean) => {
    setBusy(s.id);
    setErrors((e) => ({ ...e, [s.id]: "" }));
    try {
      if (revert) await api.defaultsRevert(s.id);
      else if ((await api.defaultsApply(s.id)) === "use_settings") await openUrl(s.settings_uri);
    } catch (e) {
      setErrors((x) => ({ ...x, [s.id]: String(e) }));
    }
    await load();
    setBusy(null);
  };

  if (!states?.length && !errors.load) return null;
  return (
    <section className="mt-8">
      <h2 className="section-title">{t("tweaks.defaults")}</h2>
      <p className="mb-4 text-[13px] text-[var(--secondary)]">{t("tweaks.defaultsHint")}</p>
      {errors.load && <p className="selectable text-[12px] text-[var(--red)]">{errors.load}</p>}
      <div className="grid grid-cols-[repeat(auto-fill,minmax(340px,1fr))] gap-4">
        {states?.map((s) => {
          const [n, total] = s.detail.split("/");
          const now =
            s.kind === "types" ? t("tweaks.types.detail", { n, total }) : s.detail === "app" ? s.name : t("tweaks.print_screen.snipping");
          const desc =
            s.kind === "print_screen"
              ? t("tweaks.print_screen.desc", { name: s.name })
              : t(boot.os_build >= 22621 ? "tweaks.types.desc" : "tweaks.types.desc10", { name: s.name });
          const err = errors[s.id];
          return (
            <Card key={s.id} className="flex flex-col gap-3 p-4">
              <div className="flex items-start gap-3">
                <AppIcon id={s.id.split(":")[0]} name={s.name} size={40} />
                <div className="min-w-0 flex-1">
                  <div className="font-semibold">{t(`tweaks.${s.kind}.title` as Key, { name: s.name })}</div>
                  <p className="text-[13px] text-[var(--secondary)]">{desc}</p>
                  {s.kind === "types" && s.home && !s.applied && <p className="mt-1 text-[12px] text-[var(--orange)]">{t("tweaks.types.home")}</p>}
                </div>
              </div>
              {err && <p className="selectable text-[12px] text-[var(--red)]">{err}</p>}
              <div className="mt-auto flex flex-wrap items-center justify-between gap-2">
                <span className="whitespace-nowrap text-[13px] text-[var(--secondary)]">
                  {!s.installed ? t("tweaks.notInstalled", { name: s.name }) : s.pending ? <Badge tone="orange">{t("tweaks.pending")}</Badge> : t("tweaks.current", { v: now })}
                </span>
                <div className="flex gap-2">
                  {s.can_revert && (
                    <Button variant="plain" disabled={busy !== null} onClick={() => run(s, true)}>
                      {t("tweaks.revert")}
                    </Button>
                  )}
                  {s.installed && !s.applied && (
                    <Button variant="plain" onClick={() => openUrl(s.settings_uri)}>{t("tweaks.openSettings")}</Button>
                  )}
                  {busy === s.id ? (
                    <Button disabled><ProgressRing /></Button>
                  ) : s.applied ? (
                    <Button variant="success" disabled title={t("tweaks.applied")}>
                      <Check size={15} strokeWidth={3} /> {t("tweaks.applied")}
                    </Button>
                  ) : (
                    // While pending, applying again would only push it once more at the same sign-in.
                    !s.pending && (
                      <Button variant="primary" disabled={!s.installed || busy !== null} onClick={() => run(s, false)}>
                        {t("tweaks.apply")}
                      </Button>
                    )
                  )}
                </div>
              </div>
            </Card>
          );
        })}
      </div>
    </section>
  );
}
