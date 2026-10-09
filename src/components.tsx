import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Check, RotateCw, X } from "lucide-react";
import { ButtonHTMLAttributes, ReactNode, useEffect, useRef, useState } from "react";
import { Job, isActive } from "./api";
import { useT } from "./i18n";

export const spring = { type: "spring", stiffness: 420, damping: 32, mass: 0.8 } as const;

type Variant = "primary" | "secondary" | "plain" | "danger" | "success";
export function Button({ variant = "secondary", className = "", children, ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  const styles: Record<Variant, string> = {
    primary: "bg-[var(--accent)] text-[var(--accent-text)] hover:brightness-110 active:brightness-95",
    secondary: "bg-[var(--fill)] text-[var(--link)] hover:bg-[var(--fill-hover)]",
    plain: "text-[var(--link)] hover:bg-[var(--fill)]",
    danger: "bg-[var(--fill)] text-[var(--red)] hover:bg-[var(--fill-hover)]",
    success: "bg-[var(--fill)] text-[var(--green)] hover:bg-[var(--fill-hover)]",
  };
  return (
    <button
      {...p}
      className={`inline-flex h-8 min-w-[84px] shrink-0 cursor-pointer items-center justify-center gap-1.5 rounded-full px-4 text-[13px] font-semibold transition-[filter,background-color] duration-150 disabled:cursor-default disabled:opacity-45 ${styles[variant]} ${className}`}
    >
      {children}
    </button>
  );
}

export function ProgressRing({ value, size = 18 }: { value?: number | null; size?: number }) {
  const r = (size - 3) / 2;
  const c = 2 * Math.PI * r;
  const indeterminate = value == null;
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className={indeterminate ? "animate-spin" : ""} aria-hidden>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="currentColor" strokeOpacity={0.2} strokeWidth={2.5} />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={r}
        fill="none"
        stroke="currentColor"
        strokeWidth={2.5}
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={indeterminate ? c * 0.7 : c * (1 - Math.min(100, value) / 100)}
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
        style={{ transition: "stroke-dashoffset 200ms ease-out" }}
      />
    </svg>
  );
}

export function Card({ children, className = "", lift = true }: { children: ReactNode; className?: string; lift?: boolean }) {
  const reduce = useReducedMotion();
  return (
    <motion.div
      layout={!reduce}
      whileHover={lift && !reduce ? { y: -2, boxShadow: "var(--shadow-hover)" } : undefined}
      transition={spring}
      className={`relative rounded-2xl border border-[var(--card-border)] bg-[var(--card)] focus-within:z-10 hover:z-10 ${className}`}
      style={{ boxShadow: "var(--shadow)" }}
    >
      {children}
    </motion.div>
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-[24px] w-[40px] shrink-0 cursor-pointer rounded-full transition-colors duration-200 ${checked ? "bg-[var(--green)]" : "bg-[var(--fill-hover)]"}`}
    >
      <motion.span
        layout
        transition={spring}
        className="absolute top-[2px] h-5 w-5 rounded-full bg-white shadow"
        style={{ left: checked ? 18 : 2 }}
      />
    </button>
  );
}

export function PageHeader({ title, subtitle, children }: { title: string; subtitle?: string; children?: ReactNode }) {
  return (
    <header className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="large-title">{title}</h1>
        {subtitle && <p className="mt-1 text-[15px] text-[var(--secondary)]">{subtitle}</p>}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </header>
  );
}

export function Badge({ children, tone = "neutral" }: { children: ReactNode; tone?: "neutral" | "orange" | "green" | "red" }) {
  const c = { neutral: "var(--secondary)", orange: "var(--orange)", green: "var(--green)", red: "var(--red)" }[tone];
  return (
    <span className="inline-flex items-center rounded-full bg-[var(--fill)] px-2 py-0.5 text-[11px] font-semibold" style={{ color: c }}>
      {children}
    </span>
  );
}

const ICON_IDS = new Set(Object.keys(import.meta.glob("/public/icons/*.png")).map((p) => p.split("/").pop()!.replace(".png", "")));
export function AppIcon({ id, name, size = 44 }: { id: string; name: string; size?: number }) {
  const [broken, setBroken] = useState(!ICON_IDS.has(id));
  if (broken) {
    const hue = [...id].reduce((a, c) => a + c.charCodeAt(0), 0) % 360;
    return (
      <div
        aria-hidden
        className="grid shrink-0 place-items-center rounded-[22%] text-[17px] font-bold text-white"
        style={{ width: size, height: size, background: `linear-gradient(180deg, hsl(${hue} 70% 58%), hsl(${hue} 70% 46%))` }}
      >
        {name[0]}
      </div>
    );
  }
  return <img src={`/icons/${id}.png`} alt="" width={size} height={size} onError={() => setBroken(true)} className="shrink-0 rounded-[22%] object-contain" draggable={false} />;
}

const PHASE_KEY = { queued: "btn.queued", resolving: "btn.resolving", verifying: "btn.verifying", installing: "btn.installing" } as const;

/** The single state button used everywhere: Install → ring % → spinner → ✓ Installed (→ Reinstall), or red Retry + Details. */
export function JobButton({
  job,
  done,
  doneLabel,
  idleLabel,
  redoLabel,
  onStart,
  onCancel,
  disabled,
}: {
  job?: Job;
  done: boolean;
  doneLabel?: string;
  idleLabel?: string;
  redoLabel?: string;
  onStart: () => void;
  onCancel?: () => void;
  disabled?: boolean;
}) {
  const { t } = useT();
  const [confirmRedo, setConfirmRedo] = useState(false);
  const [showErr, setShowErr] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!showErr && !confirmRedo) return;
    const h = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && (setShowErr(false), setConfirmRedo(false));
    window.addEventListener("mousedown", h);
    return () => window.removeEventListener("mousedown", h);
  }, [showErr, confirmRedo]);

  const active = isActive(job);
  let content: ReactNode;
  if (active) {
    const label =
      job!.phase === "downloading"
        ? job!.progress != null
          ? `${Math.floor(job!.progress)}%`
          : t("btn.download")
        : t(PHASE_KEY[job!.phase as keyof typeof PHASE_KEY] ?? "btn.installing");
    content = (
      <Button variant="secondary" onClick={onCancel} aria-label={`${label}. ${t("common.cancel")}`} title={t("common.cancel")} className="group min-w-[112px]">
        <ProgressRing value={job!.phase === "downloading" ? job!.progress : null} />
        <span className="tabular-nums group-hover:hidden">{label}</span>
        <span className="hidden group-hover:inline">{t("common.cancel")}</span>
      </Button>
    );
  } else if (job?.phase === "failed") {
    content = (
      <div className="flex items-center gap-1">
        <Button variant="danger" onClick={onStart}>
          <RotateCw size={14} /> {t("common.retry")}
        </Button>
        <Button variant="plain" className="min-w-0 px-2" onClick={() => setShowErr((v) => !v)} aria-expanded={showErr}>
          {t("common.details")}
        </Button>
      </div>
    );
  } else if (done || job?.phase === "done") {
    content = confirmRedo ? (
      <Button variant="primary" onClick={() => (setConfirmRedo(false), onStart())}>
        {redoLabel ?? t("btn.reinstall")}
      </Button>
    ) : (
      <Button variant="success" onClick={() => setConfirmRedo(true)} title={redoLabel ?? t("btn.reinstall")}>
        <Check size={15} strokeWidth={3} /> {doneLabel ?? t("btn.installed")}
      </Button>
    );
  } else {
    content = (
      <Button variant="primary" onClick={onStart} disabled={disabled}>
        {idleLabel ?? t("btn.install")}
      </Button>
    );
  }

  return (
    <div ref={ref} className="relative">
      {content}
      <AnimatePresence>
        {showErr && job?.phase === "failed" && (
          <motion.div
            role="dialog"
            initial={{ opacity: 0, y: -4, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -4, scale: 0.98 }}
            transition={spring}
            className="selectable absolute right-0 top-10 z-30 w-80 rounded-xl border border-[var(--card-border)] bg-[var(--sheet)] p-3 text-[13px] backdrop-blur-xl"
            style={{ boxShadow: "var(--shadow-hover)" }}
          >
            <div className="mb-1 flex items-center justify-between">
              <span className="font-semibold text-[var(--red)]">{t("common.error")}</span>
              <button className="cursor-pointer rounded p-0.5 text-[var(--secondary)] hover:bg-[var(--fill)]" onClick={() => setShowErr(false)} aria-label={t("common.close")}>
                <X size={14} />
              </button>
            </div>
            <p className="max-h-48 overflow-auto whitespace-pre-wrap break-words text-[var(--secondary)]">{job.message}</p>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

export function Row({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`flex items-center justify-between gap-4 px-4 py-3 ${className}`}>{children}</div>;
}
