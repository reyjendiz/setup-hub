import { Check, Gauge, Monitor, Moon, MousePointer2, Power } from "lucide-react";
import { useEffect, useState } from "react";
import { api, TweakState } from "../api";
import { Badge, Button, Card, PageHeader, ProgressRing } from "../components";
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
    </>
  );
}
