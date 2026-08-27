import type { ReactNode } from "react";

import { useI18n } from "../i18n";
import type { Level, TargetKind } from "../types";

const levelTone: Record<Level, string> = {
  ok: "border-good/40 bg-good/10 text-good",
  warn: "border-caution/40 bg-caution/10 text-caution",
  critical: "border-bad/40 bg-bad/10 text-bad",
  unknown: "border-hairline bg-surface-sunken text-ink-faint",
};

const levelDot: Record<Level, string> = {
  ok: "bg-good",
  warn: "bg-caution",
  critical: "bg-bad",
  unknown: "bg-ink-faint",
};

export function LevelPill({ level }: { level: Level }) {
  const { t } = useI18n();
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium ${levelTone[level]}`}
    >
      <span className={`size-1.5 rounded-full ${levelDot[level]}`} />
      {t(`level.${level}`)}
    </span>
  );
}

export function LevelDot({ level, paused }: { level: Level; paused?: boolean }) {
  return (
    <span
      className={`size-2.5 shrink-0 rounded-full ${paused ? "bg-ink-faint/50" : levelDot[level]}`}
    />
  );
}

export function KindTag({ kind }: { kind: TargetKind }) {
  const { t } = useI18n();
  return (
    <span className="rounded-md bg-surface-sunken px-2 py-0.5 text-xs text-ink-muted">
      {t(`kind.${kind}`)}
    </span>
  );
}

export function Button({
  children,
  onClick,
  variant = "ghost",
  disabled,
  type = "button",
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "ghost" | "danger";
  disabled?: boolean;
  type?: "button" | "submit";
}) {
  const tone = {
    primary: "bg-brand text-surface hover:brightness-110",
    ghost: "border border-hairline text-ink-muted hover:border-brand/60 hover:text-ink",
    danger: "border border-bad/40 text-bad hover:bg-bad/10",
  }[variant];

  return (
    <button
      type={type}
      onClick={onClick}
      disabled={disabled}
      className={`rounded-lg px-3 py-1.5 text-sm font-medium transition disabled:cursor-not-allowed disabled:opacity-45 ${tone}`}
    >
      {children}
    </button>
  );
}

export function Meter({
  label,
  percent,
  caption,
}: {
  label: string;
  percent: number;
  caption?: string;
}) {
  const tone = percent >= 90 ? "bg-bad" : percent >= 75 ? "bg-caution" : "bg-good";
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between gap-2 text-xs">
        <span className="text-ink-muted">{label}</span>
        <span className="font-mono text-ink">{percent.toFixed(0)}%</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-surface-sunken">
        <div
          className={`h-full rounded-full ${tone}`}
          style={{ width: `${Math.min(100, Math.max(2, percent))}%` }}
        />
      </div>
      {caption ? <p className="mt-1 font-mono text-[11px] text-ink-faint">{caption}</p> : null}
    </div>
  );
}

export function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <p className="text-xs text-ink-muted">{label}</p>
      <p className="font-mono text-sm text-ink">{value}</p>
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  hint,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  hint?: string;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="mt-0.5 size-4 shrink-0 accent-brand"
      />
      <span>
        <span className="text-sm text-ink">{label}</span>
        {hint ? <span className="hint block">{hint}</span> : null}
      </span>
    </label>
  );
}

export function bytes(value: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit + 1 < units.length) {
    size /= 1024;
    unit += 1;
  }
  return `${unit === 0 ? size : size.toFixed(1)} ${units[unit]}`;
}

export function duration(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  if (days > 0) return `${days}d ${hours}h`;
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${minutes}m`;
}

export function interval(seconds: number): string {
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  if (seconds % 60 === 0) return `${seconds / 60}m`;
  return `${seconds}s`;
}

export function lines(value: string): string[] {
  return value
    .split("\n")
    .map((entry) => entry.trim())
    .filter(Boolean);
}
