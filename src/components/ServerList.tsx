import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { useI18n } from "../i18n";
import type {
  DnsReport,
  DomainReport,
  Finding,
  NodeResult,
  ServerReport,
  TargetStatus,
  TargetView,
  WebsiteReport,
} from "../types";
import {
  Button,
  KindTag,
  LevelDot,
  LevelPill,
  Meter,
  Stat,
  bytes,
  duration,
  interval,
} from "./primitives";

interface Props {
  targets: TargetView[];
  statuses: Record<string, TargetStatus>;
  checking: string[];
  onEdit: (target: TargetView) => void;
  onDelete: (target: TargetView) => void;
  onToggle: (target: TargetView) => void;
  onCheck: (target: TargetView) => void;
}

export function ServerList({
  targets,
  statuses,
  checking,
  onEdit,
  onDelete,
  onToggle,
  onCheck,
}: Props) {
  const { t, relative } = useI18n();
  const [expanded, setExpanded] = useState<string | null>(null);

  if (targets.length === 0) {
    return (
      <div className="card flex flex-col items-center gap-2 px-6 py-16 text-center">
        <p className="text-base text-ink">{t("list.empty")}</p>
        <p className="max-w-sm text-sm text-ink-faint">{t("list.emptyHint")}</p>
      </div>
    );
  }

  return (
    <ul className="space-y-2">
      {targets.map((target) => {
        const status = statuses[target.id];
        const level = status?.level ?? "unknown";
        const open = expanded === target.id;

        return (
          <li key={target.id} className="card overflow-hidden">
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3">
              <button
                type="button"
                onClick={() => setExpanded(open ? null : target.id)}
                className="flex min-w-0 flex-1 items-center gap-3 text-start"
              >
                <LevelDot level={level} paused={!target.enabled} />
                <span className="min-w-0">
                  <span className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium text-ink">{target.name}</span>
                    <KindTag kind={target.spec.kind} />
                    {!target.enabled ? (
                      <span className="text-xs text-ink-faint">{t("list.paused")}</span>
                    ) : null}
                  </span>
                  <span className="mt-0.5 block truncate font-mono text-xs text-ink-faint">
                    {endpointOf(target)}
                  </span>
                </span>
              </button>

              <div className="flex items-center gap-3">
                <span className="hidden text-xs text-ink-faint sm:block">
                  {status ? relative(status.checkedAt) : t("status.never")}
                </span>
                <LevelPill level={level} />
                <Button
                  onClick={() => onCheck(target)}
                  disabled={checking.includes(target.id)}
                >
                  {t("action.checkNow")}
                </Button>
              </div>
            </div>

            {open ? (
              <div className="border-t border-hairline bg-surface-sunken/60 px-4 py-4">
                <Findings findings={status?.findings ?? []} />

                {status?.website ? <WebsiteDetail report={status.website} /> : null}
                {status?.dns ? <DnsDetail report={status.dns} /> : null}
                {status?.server ? <ServerDetail report={status.server} /> : null}
                {status?.domain ? <DomainDetail report={status.domain} /> : null}

                <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-hairline pt-3">
                  <p className="font-mono text-xs text-ink-faint">
                    {status ? <Freshness status={status} /> : t("status.never")} ·{" "}
                    {interval(target.intervalSecs)}
                  </p>
                  <div className="flex gap-2">
                    <Button onClick={() => onToggle(target)}>
                      {target.enabled ? t("action.pause") : t("action.resume")}
                    </Button>
                    <Button onClick={() => onEdit(target)}>{t("action.edit")}</Button>
                    <Button variant="danger" onClick={() => onDelete(target)}>
                      {t("action.delete")}
                    </Button>
                  </div>
                </div>
              </div>
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

function Freshness({ status }: { status: TargetStatus }) {
  const { t, relative } = useI18n();
  return (
    <>
      {t("status.lastChecked", { ago: relative(status.checkedAt) })} ·{" "}
      {t("status.took", { ms: status.durationMs })}
    </>
  );
}

function Findings({ findings }: { findings: Finding[] }) {
  const { finding: translate } = useI18n();
  if (findings.length === 0) return null;

  const tone = {
    critical: "border-bad/50 bg-bad/5",
    warn: "border-caution/50 bg-caution/5",
    ok: "border-good/40 bg-good/5",
    unknown: "border-hairline",
  };

  return (
    <ul className="space-y-1.5">
      {findings.map((item, index) => (
        <li
          key={`${item.code}-${index}`}
          className={`rounded-lg border px-3 py-2 ${tone[item.level]}`}
        >
          <p className="text-sm text-ink">{translate(item.code)}</p>
          {item.detail ? (
            <p className="mt-0.5 font-mono text-xs leading-relaxed text-ink-muted">{item.detail}</p>
          ) : null}
        </li>
      ))}
    </ul>
  );
}

function WebsiteDetail({ report }: { report: WebsiteReport }) {
  const { t, relative } = useI18n();
  const inside = report.external?.nodes.filter((node) => node.country.toLowerCase() === "ir") ?? [];
  const abroad = report.external?.nodes.filter((node) => node.country.toLowerCase() !== "ir") ?? [];

  return (
    <div className="mt-4 space-y-4">
      <section>
        <h4 className="label">{t("status.localNetwork")}</h4>
        <p className="font-mono text-sm text-ink">
          {report.local.reachable
            ? `HTTP ${report.local.status ?? "?"} · ${report.local.latencyMs ?? "?"} ms`
            : (report.local.error ?? t("status.unreachable"))}
        </p>
      </section>

      {report.external ? (
        <section>
          <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2">
            <h4 className="label mb-0">{t("status.external")}</h4>
            <span className="text-xs text-ink-faint">
              {t("status.externalAge", { ago: relative(report.external.checkedAt) })}
            </span>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <NodeGroup title="🇮🇷" nodes={inside} />
            <NodeGroup title="🌍" nodes={abroad} />
          </div>
          {report.external.permanentLink ? (
            <button
              type="button"
              onClick={() => void openUrl(report.external!.permanentLink!)}
              className="mt-2 text-xs text-brand hover:underline"
            >
              {t("action.report")}
            </button>
          ) : null}
        </section>
      ) : (
        <p className="text-xs text-ink-faint">{t("status.noExternal")}</p>
      )}
    </div>
  );
}

function NodeGroup({ title, nodes }: { title: string; nodes: NodeResult[] }) {
  const { t } = useI18n();
  if (nodes.length === 0) return null;

  return (
    <ul className="space-y-1">
      {nodes.map((node) => (
        <li key={node.node} className="flex items-center justify-between gap-2 text-xs">
          <span className="flex min-w-0 items-center gap-2">
            <span aria-hidden="true">{title}</span>
            <span className="truncate text-ink-muted">
              {node.city || node.country.toUpperCase()}
              <span className="text-ink-faint"> {node.asn}</span>
            </span>
          </span>
          <span
            className={`shrink-0 font-mono ${
              node.reachable === null
                ? "text-ink-faint"
                : node.reachable
                  ? "text-good"
                  : "text-bad"
            }`}
          >
            {node.reachable === null
              ? t("status.pendingNode")
              : node.reachable
                ? `${node.status ?? "ok"} · ${node.latencyMs ?? "?"} ms`
                : (node.message ?? t("status.unreachable"))}
          </span>
        </li>
      ))}
    </ul>
  );
}

function DnsDetail({ report }: { report: DnsReport }) {
  const { t, relative } = useI18n();
  if (report.attempts.length === 0) return null;

  return (
    <section className="mt-4">
      <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2">
        <h4 className="label mb-0">{t("status.dns")}</h4>
        <span className="text-xs text-ink-faint">
          {t("status.externalAge", { ago: relative(report.checkedAt) })}
        </span>
      </div>
      <ul className="space-y-1">
        {report.attempts.map((attempt) => {
          const failed = attempt.error !== null;
          const serves = attempt.probe?.reachable === true;
          return (
            <li
              key={attempt.resolver}
              className="flex items-start justify-between gap-3 text-xs"
            >
              <span className="flex min-w-0 flex-col">
                <span className="truncate text-ink-muted">
                  {attempt.label}
                  {attempt.builtin ? null : (
                    <span className="text-ink-faint"> · {t("status.dns.custom")}</span>
                  )}
                </span>
                <span className="truncate font-mono text-ink-faint">{attempt.resolver}</span>
              </span>
              <span
                className={`shrink-0 text-end font-mono ${
                  failed
                    ? "text-ink-faint"
                    : attempt.reserved
                      ? "text-bad"
                      : serves
                        ? "text-good"
                        : "text-caution"
                }`}
              >
                {failed ? (
                  attempt.error
                ) : (
                  <>
                    {attempt.answers.join(", ")}
                    <span className="block">
                      {attempt.probe
                        ? serves
                          ? `HTTP ${attempt.probe.status ?? "?"} · ${attempt.probe.latencyMs ?? "?"} ms`
                          : (attempt.probe.error ?? t("status.unreachable"))
                        : t("status.dns.noAddress")}
                    </span>
                  </>
                )}
              </span>
            </li>
          );
        })}
      </ul>
      <p className="hint mt-2">{t("status.dns.hint")}</p>
    </section>
  );
}

function ServerDetail({ report }: { report: ServerReport }) {
  const { t } = useI18n();
  return (
    <div className="mt-4 space-y-4">
      <div className="grid gap-4 sm:grid-cols-3">
        {report.loadPerCore !== null ? (
          <Stat
            label={t("server.load")}
            value={t("server.perCore", { value: report.loadPerCore.toFixed(2) })}
          />
        ) : null}
        {report.uptimeSecs !== null ? (
          <Stat label={t("server.uptime")} value={duration(report.uptimeSecs)} />
        ) : null}
        {report.cores !== null ? <Stat label="CPU" value={`${report.cores}×`} /> : null}
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        {report.memory ? (
          <Meter
            label={t("server.memory")}
            percent={report.memory.percent}
            caption={`${bytes(report.memory.used)} / ${bytes(report.memory.total)}`}
          />
        ) : null}
        {report.disks.map((disk) => (
          <Meter
            key={disk.mount}
            label={`${t("server.disk")} ${disk.mount}`}
            percent={disk.percent}
            caption={`${bytes(disk.used)} / ${bytes(disk.total)}`}
          />
        ))}
      </div>

      {report.containers.length > 0 ? (
        <section>
          <h4 className="label">{t("server.containers")}</h4>
          <ul className="flex flex-wrap gap-1.5">
            {report.containers.map((container) => (
              <li
                key={container.name}
                title={container.status}
                className={`rounded-md border px-2 py-0.5 font-mono text-xs ${
                  container.state === "running" && container.health !== "unhealthy"
                    ? "border-good/40 text-good"
                    : "border-bad/40 text-bad"
                }`}
              >
                {container.name}
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {report.hostFingerprint ? (
        <p className="truncate font-mono text-[11px] text-ink-faint">
          {t("server.fingerprint")} {report.hostFingerprint}
        </p>
      ) : null}
    </div>
  );
}

function DomainDetail({ report }: { report: DomainReport }) {
  const { t, date } = useI18n();
  return (
    <div className="mt-4 grid gap-4 sm:grid-cols-2">
      {report.certificate ? (
        <Stat
          label={t("domain.certificate")}
          value={
            <>
              {t("domain.daysLeft", { days: report.certificate.daysLeft })}
              <span className="block text-xs text-ink-faint">
                {t("domain.expires", { date: date(report.certificate.notAfter) })} ·{" "}
                {t("domain.issuer", { issuer: report.certificate.issuer })}
              </span>
            </>
          }
        />
      ) : null}
      {report.registration ? (
        <Stat
          label={t("domain.registration")}
          value={
            <>
              {t("domain.daysLeft", { days: report.registration.daysLeft })}
              <span className="block text-xs text-ink-faint">
                {t("domain.expires", { date: date(report.registration.expiresAt) })} ·{" "}
                {report.registration.source}
              </span>
            </>
          }
        />
      ) : null}
    </div>
  );
}

function endpointOf(target: TargetView): string {
  switch (target.spec.kind) {
    case "website":
      return target.spec.url;
    case "server":
      return `${target.spec.username}@${target.spec.host}:${target.spec.port}`;
    case "domain":
      return target.spec.domain;
  }
}
