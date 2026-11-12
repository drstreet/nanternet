import { useI18n } from "../i18n";
import type { Level, TargetStatus, TargetView } from "../types";
import { Button } from "./primitives";
import { ServerList } from "./ServerList";

interface Props {
  targets: TargetView[];
  statuses: Record<string, TargetStatus>;
  checking: string[];
  onAdd: () => void;
  onEdit: (target: TargetView) => void;
  onDelete: (target: TargetView) => void;
  onToggle: (target: TargetView) => void;
  onCheck: (target: TargetView) => void;
  onCheckAll: () => void;
  silencedUntil: number | null;
  onSilence: (until: number | null) => void;
  onSnooze: (target: TargetView, until: number | null) => void;
}

export function Dashboard({
  targets,
  statuses,
  checking,
  onAdd,
  onEdit,
  onDelete,
  onToggle,
  onCheck,
  onCheckAll,
  silencedUntil,
  onSilence,
  onSnooze,
}: Props) {
  const { t, clock } = useI18n();
  const silenced = silencedUntil !== null && silencedUntil > Date.now() / 1000;

  const counts = targets.reduce(
    (totals, target) => {
      const level: Level = target.enabled ? (statuses[target.id]?.level ?? "unknown") : "unknown";
      totals[level] += 1;
      return totals;
    },
    { ok: 0, warn: 0, critical: 0, unknown: 0 } as Record<Level, number>,
  );

  const cards = [
    { key: "critical", value: counts.critical, tone: "text-bad" },
    { key: "warn", value: counts.warn, tone: "text-caution" },
    { key: "ok", value: counts.ok, tone: "text-good" },
    { key: "pending", value: counts.unknown, tone: "text-ink-faint" },
  ] as const;

  return (
    <div className="space-y-5">
      {silenced ? (
        <div className="card flex flex-wrap items-center justify-between gap-3 border-caution/40 bg-caution/5 px-4 py-3">
          <p className="text-sm text-ink">
            {t("snooze.allUntil", { time: clock(silencedUntil) })}
          </p>
          <Button onClick={() => onSilence(null)}>{t("snooze.resume")}</Button>
        </div>
      ) : null}

      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="grid flex-1 grid-cols-2 gap-2 sm:grid-cols-4">
          {cards.map((card) => (
            <div key={card.key} className="card px-4 py-3">
              <p className="text-xs text-ink-muted">{t(`summary.${card.key}`)}</p>
              <p className={`mt-0.5 text-2xl font-semibold tabular-nums ${card.tone}`}>
                {card.value}
              </p>
            </div>
          ))}
        </div>
        <div className="flex gap-2">
          {!silenced ? (
            <Button onClick={() => onSilence(Math.floor(Date.now() / 1000) + 3600)}>
              {t("snooze.allHour")}
            </Button>
          ) : null}
          <Button onClick={onCheckAll} disabled={targets.length === 0}>
            {t("action.checkAll")}
          </Button>
          <Button variant="primary" onClick={onAdd}>
            {t("action.add")}
          </Button>
        </div>
      </div>

      <ServerList
        targets={targets}
        statuses={statuses}
        checking={checking}
        onEdit={onEdit}
        onDelete={onDelete}
        onToggle={onToggle}
        onCheck={onCheck}
        onSnooze={onSnooze}
      />
    </div>
  );
}
