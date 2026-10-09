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
export function AppIcon({ id, name, size = 44, src }: { id: string; name: string; size?: number; src?: string | null }) {
  const [broken, setBroken] = useState(!src && !ICON_IDS.has(id));
  useEffect(() => setBroken(!src && !ICON_IDS.has(id)), [src, id]);
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
  return <img src={src ?? `/icons/${id}.png`} alt="" width={size} height={size} onError={() => setBroken(true)} className="shrink-0 rounded-[22%] object-contain" draggable={false} />;
}

/** Modal sheet: blurred backdrop, rounded panel, Esc / backdrop click closes, focus moves inside. */
export function Sheet({ title, onClose, children, footer, width = 560 }: { title: string; onClose: () => void; children: ReactNode; footer?: ReactNode; width?: number }) {
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    panel.current?.querySelector<HTMLElement>("textarea,input,select,button:not([data-close])")?.focus();
    const h = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", h);
    return () => (window.removeEventListener("keydown", h), prev?.focus());
  }, [onClose]);
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.15 }}
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/25 pt-[8vh] backdrop-blur-sm"
      onMouseDown={onClose}
    >
      <motion.div
        ref={panel}
        initial={{ scale: 0.97, y: 12 }}
        animate={{ scale: 1, y: 0 }}
        exit={{ scale: 0.97, y: 12 }}
        transition={spring}
        role="dialog"
        aria-modal
        aria-label={title}
        onMouseDown={(e) => e.stopPropagation()}
        className="flex max-h-[84vh] w-[min(var(--w),92vw)] flex-col overflow-hidden rounded-2xl border border-[var(--card-border)] bg-[var(--sheet)] backdrop-blur-xl"
        style={{ boxShadow: "0 24px 80px rgba(0,0,0,.3)", ["--w" as string]: `${width}px` }}
      >
        <div className="flex items-center justify-between px-5 pb-2 pt-4">
          <h2 className="section-title">{title}</h2>
          <button data-close onClick={onClose} aria-label="Close" className="cursor-pointer rounded-full p-1.5 text-[var(--secondary)] hover:bg-[var(--fill)]">
            <X size={16} />
          </button>
        </div>
        <div className="scroll min-h-0 flex-1 px-5 pb-4 pt-1">{children}</div>
        {footer && <div className="flex items-center justify-end gap-2 border-t border-[var(--separator)] px-5 py-3">{footer}</div>}
      </motion.div>
    </motion.div>
  );
}

export interface MenuItem {
  label: string;
  onSelect: () => void;
  checked?: boolean;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
}

/** Context menu at a screen point; arrow keys move, Esc / outside click closes. */
export function Menu({ at, items, onClose }: { at: { x: number; y: number }; items: MenuItem[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState(at);
  useEffect(() => {
    const el = ref.current!;
    const r = el.getBoundingClientRect();
    setPos({ x: Math.min(at.x, window.innerWidth - r.width - 8), y: Math.min(at.y, window.innerHeight - r.height - 8) });
    el.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
    const down = (e: MouseEvent) => !el.contains(e.target as Node) && onClose();
    const key = (e: KeyboardEvent) => {
      const btns = [...el.querySelectorAll<HTMLElement>("button:not(:disabled)")];
      const i = btns.indexOf(document.activeElement as HTMLElement);
      if (e.key === "Escape") onClose();
      if (e.key === "ArrowDown") (e.preventDefault(), btns[(i + 1) % btns.length]?.focus());
      if (e.key === "ArrowUp") (e.preventDefault(), btns[(i - 1 + btns.length) % btns.length]?.focus());
    };
    window.addEventListener("mousedown", down);
    window.addEventListener("keydown", key);
    return () => (window.removeEventListener("mousedown", down), window.removeEventListener("keydown", key));
  }, [at, onClose]);
  return (
    <motion.div
      ref={ref}
      role="menu"
      initial={{ opacity: 0, scale: 0.97 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{ duration: 0.12 }}
      className="fixed z-[60] min-w-[220px] rounded-xl border border-[var(--card-border)] bg-[var(--sheet)] p-1 text-[13px] backdrop-blur-xl"
      style={{ left: pos.x, top: pos.y, boxShadow: "var(--shadow-hover)" }}
    >
      {items.map((it, i) =>
        it.separator ? (
          <div key={i} className="my-1 h-px bg-[var(--separator)]" />
        ) : (
          <button
            key={i}
            role={it.checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-checked={it.checked}
            disabled={it.disabled}
            onClick={() => (onClose(), it.onSelect())}
            className={`flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 text-left outline-none hover:bg-[var(--accent)] hover:text-[var(--accent-text)] focus:bg-[var(--accent)] focus:text-[var(--accent-text)] disabled:cursor-default disabled:opacity-40 ${it.danger ? "text-[var(--red)]" : ""}`}
          >
            <span className="w-4">{it.checked && <Check size={14} strokeWidth={3} />}</span>
            {it.label}
          </button>
        ),
      )}
    </motion.div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[12px] font-medium text-[var(--secondary)]">{label}</span>
      {children}
      {hint && <span className="text-[11px] text-[var(--tertiary)]">{hint}</span>}
    </label>
  );
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
  onReview,
  disabled,
}: {
  job?: Job;
  done: boolean;
  doneLabel?: string;
  idleLabel?: string;
  redoLabel?: string;
  onStart: () => void;
  onCancel?: () => void;
  onReview?: () => void;
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
  } else if (job?.phase === "review" && onReview) {
    content = (
      <Button variant="secondary" className="!text-[var(--orange)]" onClick={onReview}>
        {t("my.review")}
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

export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: [T, ReactNode][]; onChange: (v: T) => void; label: string }) {
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
