import { useEffect, useMemo, useState } from "react";

import { useI18n } from "../i18n";
import type { AlertLevel, Target, TargetKind, TargetSpec, TargetView } from "../types";
import { Button, Toggle, lines } from "./primitives";
import { ALERT_LEVELS } from "./Settings";

const INTERVALS = [30, 60, 120, 300, 600, 1800, 3600];
const EXTERNAL_INTERVALS = [300, 900, 1800, 3600, 10800];

interface Draft {
  name: string;
  enabled: boolean;
  intervalSecs: number;
  alerts: AlertLevel | "inherit";
  kind: TargetKind;
  url: string;
  externalProbe: boolean;
  externalIntervalSecs: number;
  dnsProbe: boolean;
  expectedStatus: string;
  expectedBody: string;
  iranNodes: number;
  abroadNodes: number;
  iranAlertThreshold: number;
  abroadAlertThreshold: number;
  host: string;
  port: string;
  username: string;
  authMethod: "key" | "password";
  keyPath: string;
  loadLimit: string;
  memoryLimit: string;
  diskLimit: string;
  mounts: string;
  containers: string;
  domain: string;
  domainPort: string;
  checkCertificate: boolean;
  checkRegistration: boolean;
  warnDays: string;
}

const blank: Draft = {
  name: "",
  enabled: true,
  intervalSecs: 120,
  alerts: "inherit",
  kind: "website",
  url: "",
  externalProbe: true,
  externalIntervalSecs: 900,
  dnsProbe: false,
  expectedStatus: "",
  expectedBody: "",
  iranNodes: 3,
  abroadNodes: 4,
  iranAlertThreshold: 1,
  abroadAlertThreshold: 1,
  host: "",
  port: "22",
  username: "root",
  authMethod: "key",
  keyPath: "~/.ssh/id_ed25519",
  loadLimit: "1.5",
  memoryLimit: "85",
  diskLimit: "85",
  mounts: "/",
  containers: "",
  domain: "",
  domainPort: "443",
  checkCertificate: true,
  checkRegistration: true,
  warnDays: "14",
};

function seed(target: TargetView | null): Draft {
  if (!target) return blank;
  const draft: Draft = {
    ...blank,
    name: target.name,
    enabled: target.enabled,
    intervalSecs: target.intervalSecs,
    alerts: target.alerts ?? "inherit",
    kind: target.spec.kind,
  };

  if (target.spec.kind === "website") {
    const spec = target.spec;
    return {
      ...draft,
      url: spec.url,
      externalProbe: spec.externalProbe,
      externalIntervalSecs: spec.externalIntervalSecs,
      dnsProbe: spec.dnsProbe,
      expectedStatus: spec.expectedStatus ? String(spec.expectedStatus) : "",
      expectedBody: spec.expectedBody ?? "",
      iranNodes: spec.iranNodes,
      abroadNodes: spec.abroadNodes,
      iranAlertThreshold: spec.iranAlertThreshold,
      abroadAlertThreshold: spec.abroadAlertThreshold,
    };
  }

  if (target.spec.kind === "server") {
    const spec = target.spec;
    return {
      ...draft,
      host: spec.host,
      port: String(spec.port),
      username: spec.username,
      authMethod: spec.auth.method,
      keyPath: spec.auth.method === "key" ? spec.auth.path : blank.keyPath,
      loadLimit: String(spec.loadLimit),
      memoryLimit: String(spec.memoryLimit),
      diskLimit: String(spec.diskLimit),
      mounts: spec.mounts.join("\n"),
      containers: spec.containers.join("\n"),
    };
  }

  const spec = target.spec;
  return {
    ...draft,
    domain: spec.domain,
    domainPort: String(spec.port),
    checkCertificate: spec.checkCertificate,
    checkRegistration: spec.checkRegistration,
    warnDays: String(spec.warnDays),
  };
}

const thresholds = (nodes: number) => Array.from({ length: nodes + 1 }, (_, index) => index);

function number(value: string, fallback: number): number {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function specOf(draft: Draft): TargetSpec {
  switch (draft.kind) {
    case "website":
      return {
        kind: "website",
        url: draft.url,
        externalProbe: draft.externalProbe,
        externalIntervalSecs: draft.externalIntervalSecs,
        dnsProbe: draft.dnsProbe,
        expectedStatus: draft.expectedStatus ? number(draft.expectedStatus, 200) : null,
        expectedBody: draft.expectedBody.trim() || null,
        iranNodes: draft.iranNodes,
        abroadNodes: draft.abroadNodes,
        iranAlertThreshold: Math.min(draft.iranAlertThreshold, draft.iranNodes),
        abroadAlertThreshold: Math.min(draft.abroadAlertThreshold, draft.abroadNodes),
      };
    case "server":
      return {
        kind: "server",
        host: draft.host,
        port: number(draft.port, 22),
        username: draft.username,
        auth:
          draft.authMethod === "key"
            ? { method: "key", path: draft.keyPath }
            : { method: "password" },
        hostFingerprint: null,
        loadLimit: number(draft.loadLimit, 1.5),
        memoryLimit: number(draft.memoryLimit, 85),
        diskLimit: number(draft.diskLimit, 85),
        mounts: lines(draft.mounts),
        containers: lines(draft.containers),
      };
    case "domain":
      return {
        kind: "domain",
        domain: draft.domain,
        port: number(draft.domainPort, 443),
        checkCertificate: draft.checkCertificate,
        checkRegistration: draft.checkRegistration,
        warnDays: number(draft.warnDays, 14),
      };
  }
}

interface Props {
  target: TargetView | null;
  onClose: () => void;
  onSubmit: (target: Target, secret?: string) => Promise<boolean>;
}

export function AddServerModal({ target, onClose, onSubmit }: Props) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<Draft>(() => seed(target));
  const [secret, setSecret] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [onClose]);

  const patch = <K extends keyof Draft>(key: K, value: Draft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const existingFingerprint = useMemo(
    () => (target?.spec.kind === "server" ? target.spec.hostFingerprint : null),
    [target],
  );

  const submit = async () => {
    setSaving(true);
    try {
      const spec = specOf(draft);
      if (spec.kind === "server" && existingFingerprint) {
        spec.hostFingerprint = existingFingerprint;
      }
      const stored = await onSubmit(
        {
          id: target?.id ?? "",
          name: draft.name,
          enabled: draft.enabled,
          intervalSecs: draft.intervalSecs,
          alerts: draft.alerts === "inherit" ? null : draft.alerts,
          spec,
        },
        secret.length > 0 ? secret : undefined,
      );
      if (stored) onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-black/60 p-6 backdrop-blur-sm">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
        className="card my-auto w-full max-w-2xl shadow-2xl"
      >
        <header className="flex items-center justify-between border-b border-hairline px-5 py-4">
          <h2 className="text-base font-medium text-ink">
            {target ? t("action.edit") : t("action.add")}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="text-ink-faint transition hover:text-ink"
            aria-label={t("action.cancel")}
          >
            ✕
          </button>
        </header>

        <div className="space-y-5 px-5 py-5">
          {!target ? (
            <fieldset>
              <legend className="label">{t("field.kind")}</legend>
              <div className="grid gap-2 sm:grid-cols-3">
                {(["website", "server", "domain"] as const).map((kind) => (
                  <button
                    key={kind}
                    type="button"
                    onClick={() => patch("kind", kind)}
                    className={`rounded-lg border px-3 py-2.5 text-start transition ${
                      draft.kind === kind
                        ? "border-brand bg-brand/10"
                        : "border-hairline hover:border-brand/50"
                    }`}
                  >
                    <span className="block text-sm text-ink">{t(`kind.${kind}`)}</span>
                    <span className="mt-0.5 block text-[11px] leading-snug text-ink-faint">
                      {t(`kind.${kind}.hint`)}
                    </span>
                  </button>
                ))}
              </div>
            </fieldset>
          ) : null}

          <div className="grid gap-4 sm:grid-cols-2">
            <div>
              <label className="label" htmlFor="target-name">
                {t("field.name")}
              </label>
              <input
                id="target-name"
                required
                className="field"
                value={draft.name}
                onChange={(event) => patch("name", event.target.value)}
              />
            </div>
            <div>
              <label className="label" htmlFor="target-interval">
                {t("field.interval")}
              </label>
              <select
                id="target-interval"
                className="field"
                value={draft.intervalSecs}
                onChange={(event) => patch("intervalSecs", Number(event.target.value))}
              >
                {INTERVALS.map((seconds) => (
                  <option key={seconds} value={seconds}>
                    {seconds < 60
                      ? t("time.seconds", { count: seconds })
                      : seconds < 3600
                        ? t("time.minutes", { count: seconds / 60 })
                        : t("time.hours", { count: seconds / 3600 })}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {draft.kind === "website" ? (
            <div className="space-y-4">
              <div>
                <label className="label" htmlFor="target-url">
                  {t("field.url")}
                </label>
                <input
                  id="target-url"
                  required
                  className="field font-mono"
                  placeholder="https://example.ir"
                  value={draft.url}
                  onChange={(event) => patch("url", event.target.value)}
                />
              </div>

              <div className="grid gap-4 sm:grid-cols-2">
                <div>
                  <label className="label" htmlFor="target-status">
                    {t("field.expectedStatus")}
                  </label>
                  <input
                    id="target-status"
                    className="field font-mono"
                    inputMode="numeric"
                    placeholder="200"
                    value={draft.expectedStatus}
                    onChange={(event) => patch("expectedStatus", event.target.value)}
                  />
                  <p className="hint">{t("field.expectedStatus.hint")}</p>
                </div>
                <div>
                  <label className="label" htmlFor="target-body">
                    {t("field.expectedBody")}
                  </label>
                  <input
                    id="target-body"
                    className="field"
                    value={draft.expectedBody}
                    onChange={(event) => patch("expectedBody", event.target.value)}
                  />
                </div>
              </div>

              <Toggle
                checked={draft.dnsProbe}
                onChange={(value) => patch("dnsProbe", value)}
                label={t("field.dnsProbe")}
                hint={t("field.dnsProbe.hint")}
              />

              <Toggle
                checked={draft.externalProbe}
                onChange={(value) => patch("externalProbe", value)}
                label={t("field.externalProbe")}
              />

              {draft.externalProbe ? (
                <>
                  <div className="grid gap-4 sm:grid-cols-3">
                    <div>
                      <label className="label" htmlFor="target-external-interval">
                        {t("field.externalInterval")}
                      </label>
                      <select
                        id="target-external-interval"
                        className="field"
                        value={draft.externalIntervalSecs}
                        onChange={(event) =>
                          patch("externalIntervalSecs", Number(event.target.value))
                        }
                      >
                        {EXTERNAL_INTERVALS.map((seconds) => (
                          <option key={seconds} value={seconds}>
                            {seconds < 3600
                              ? t("time.minutes", { count: seconds / 60 })
                              : t("time.hours", { count: seconds / 3600 })}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <label className="label" htmlFor="target-iran-nodes">
                        {t("field.iranNodes")}
                      </label>
                      <select
                        id="target-iran-nodes"
                        className="field"
                        value={draft.iranNodes}
                        onChange={(event) => patch("iranNodes", Number(event.target.value))}
                      >
                        {[0, 1, 2, 3, 4, 5].map((count) => (
                          <option key={count} value={count}>
                            {count}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <label className="label" htmlFor="target-abroad-nodes">
                        {t("field.abroadNodes")}
                      </label>
                      <select
                        id="target-abroad-nodes"
                        className="field"
                        value={draft.abroadNodes}
                        onChange={(event) => patch("abroadNodes", Number(event.target.value))}
                      >
                        {[1, 2, 3, 4, 5, 6].map((count) => (
                          <option key={count} value={count}>
                            {count}
                          </option>
                        ))}
                      </select>
                    </div>
                    <p className="hint sm:col-span-3">{t("field.nodes.hint")}</p>
                  </div>

                  <div className="grid gap-4 sm:grid-cols-2">
                    <div>
                      <label className="label" htmlFor="target-iran-threshold">
                        {t("field.iranAlertThreshold")}
                      </label>
                      <select
                        id="target-iran-threshold"
                        className="field"
                        disabled={draft.iranNodes === 0}
                        value={Math.min(draft.iranAlertThreshold, draft.iranNodes)}
                        onChange={(event) =>
                          patch("iranAlertThreshold", Number(event.target.value))
                        }
                      >
                        {thresholds(draft.iranNodes).map((count) => (
                          <option key={count} value={count}>
                            {count === 0
                              ? t("field.alertThreshold.off")
                              : t("field.alertThreshold.count", { count })}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <label className="label" htmlFor="target-abroad-threshold">
                        {t("field.abroadAlertThreshold")}
                      </label>
                      <select
                        id="target-abroad-threshold"
                        className="field"
                        value={Math.min(draft.abroadAlertThreshold, draft.abroadNodes)}
                        onChange={(event) =>
                          patch("abroadAlertThreshold", Number(event.target.value))
                        }
                      >
                        {thresholds(draft.abroadNodes).map((count) => (
                          <option key={count} value={count}>
                            {count === 0
                              ? t("field.alertThreshold.off")
                              : t("field.alertThreshold.count", { count })}
                          </option>
                        ))}
                      </select>
                    </div>
                    <p className="hint sm:col-span-2">{t("field.alertThreshold.hint")}</p>
                  </div>
                </>
              ) : null}
            </div>
          ) : null}

          {draft.kind === "server" ? (
            <div className="space-y-4">
              <div className="grid gap-4 sm:grid-cols-[2fr_1fr_1.5fr]">
                <div>
                  <label className="label" htmlFor="target-host">
                    {t("field.host")}
                  </label>
                  <input
                    id="target-host"
                    required
                    className="field font-mono"
                    value={draft.host}
                    onChange={(event) => patch("host", event.target.value)}
                  />
                </div>
                <div>
                  <label className="label" htmlFor="target-port">
                    {t("field.port")}
                  </label>
                  <input
                    id="target-port"
                    className="field font-mono"
                    inputMode="numeric"
                    value={draft.port}
                    onChange={(event) => patch("port", event.target.value)}
                  />
                </div>
                <div>
                  <label className="label" htmlFor="target-user">
                    {t("field.username")}
                  </label>
                  <input
                    id="target-user"
                    required
                    className="field font-mono"
                    value={draft.username}
                    onChange={(event) => patch("username", event.target.value)}
                  />
                </div>
              </div>

              <fieldset>
                <legend className="label">{t("field.auth")}</legend>
                <div className="grid gap-2 sm:grid-cols-2">
                  {(["key", "password"] as const).map((method) => (
                    <button
                      key={method}
                      type="button"
                      onClick={() => patch("authMethod", method)}
                      className={`rounded-lg border px-3 py-2 text-sm transition ${
                        draft.authMethod === method
                          ? "border-brand bg-brand/10 text-ink"
                          : "border-hairline text-ink-muted hover:border-brand/50"
                      }`}
                    >
                      {t(`auth.${method}`)}
                    </button>
                  ))}
                </div>
                {draft.authMethod === "password" ? (
                  <p className="mt-2 rounded-lg border border-caution/40 bg-caution/5 px-3 py-2 text-xs leading-relaxed text-caution">
                    {t("auth.recommendKey")}
                  </p>
                ) : null}
              </fieldset>

              {draft.authMethod === "key" ? (
                <div>
                  <label className="label" htmlFor="target-key">
                    {t("field.keyPath")}
                  </label>
                  <input
                    id="target-key"
                    className="field font-mono"
                    value={draft.keyPath}
                    onChange={(event) => patch("keyPath", event.target.value)}
                  />
                </div>
              ) : null}

              <div>
                <label className="label" htmlFor="target-secret">
                  {draft.authMethod === "key" ? t("field.passphrase") : t("field.password")}
                </label>
                <input
                  id="target-secret"
                  type="password"
                  className="field font-mono"
                  autoComplete="new-password"
                  value={secret}
                  onChange={(event) => setSecret(event.target.value)}
                />
                {target?.secretStored ? (
                  <p className="hint">
                    {`${t("auth.stored")} ${t("auth.keepStored")} ${t("auth.clearStored")}`}
                  </p>
                ) : null}
              </div>

              <div className="grid gap-4 sm:grid-cols-3">
                <div>
                  <label className="label" htmlFor="target-load">
                    {t("field.loadLimit")}
                  </label>
                  <input
                    id="target-load"
                    className="field font-mono"
                    inputMode="decimal"
                    value={draft.loadLimit}
                    onChange={(event) => patch("loadLimit", event.target.value)}
                  />
                </div>
                <div>
                  <label className="label" htmlFor="target-memory">
                    {t("field.memoryLimit")} %
                  </label>
                  <input
                    id="target-memory"
                    className="field font-mono"
                    inputMode="decimal"
                    value={draft.memoryLimit}
                    onChange={(event) => patch("memoryLimit", event.target.value)}
                  />
                </div>
                <div>
                  <label className="label" htmlFor="target-disk">
                    {t("field.diskLimit")} %
                  </label>
                  <input
                    id="target-disk"
                    className="field font-mono"
                    inputMode="decimal"
                    value={draft.diskLimit}
                    onChange={(event) => patch("diskLimit", event.target.value)}
                  />
                </div>
              </div>

              <div className="grid gap-4 sm:grid-cols-2">
                <div>
                  <label className="label" htmlFor="target-mounts">
                    {t("field.mounts")}
                  </label>
                  <textarea
                    id="target-mounts"
                    rows={3}
                    className="field font-mono"
                    value={draft.mounts}
                    onChange={(event) => patch("mounts", event.target.value)}
                  />
                  <p className="hint">{t("field.mounts.hint")}</p>
                </div>
                <div>
                  <label className="label" htmlFor="target-containers">
                    {t("field.containers")}
                  </label>
                  <textarea
                    id="target-containers"
                    rows={3}
                    className="field font-mono"
                    placeholder={"nginx\npostgres"}
                    value={draft.containers}
                    onChange={(event) => patch("containers", event.target.value)}
                  />
                  <p className="hint">{t("field.containers.hint")}</p>
                </div>
              </div>
            </div>
          ) : null}

          {draft.kind === "domain" ? (
            <div className="space-y-4">
              <div className="grid gap-4 sm:grid-cols-[2fr_1fr_1fr]">
                <div>
                  <label className="label" htmlFor="target-domain">
                    {t("field.domain")}
                  </label>
                  <input
                    id="target-domain"
                    required
                    className="field font-mono"
                    placeholder="example.ir"
                    value={draft.domain}
                    onChange={(event) => patch("domain", event.target.value)}
                  />
                </div>
                <div>
                  <label className="label" htmlFor="target-domain-port">
                    {t("field.port")}
                  </label>
                  <input
                    id="target-domain-port"
                    className="field font-mono"
                    inputMode="numeric"
                    value={draft.domainPort}
                    onChange={(event) => patch("domainPort", event.target.value)}
                  />
                </div>
                <div>
                  <label className="label" htmlFor="target-warn">
                    {t("field.warnDays")}
                  </label>
                  <input
                    id="target-warn"
                    className="field font-mono"
                    inputMode="numeric"
                    value={draft.warnDays}
                    onChange={(event) => patch("warnDays", event.target.value)}
                  />
                </div>
              </div>

              <Toggle
                checked={draft.checkCertificate}
                onChange={(value) => patch("checkCertificate", value)}
                label={t("field.checkCertificate")}
              />
              <Toggle
                checked={draft.checkRegistration}
                onChange={(value) => patch("checkRegistration", value)}
                label={t("field.checkRegistration")}
              />
            </div>
          ) : null}

          <div>
            <label className="label" htmlFor="target-alerts">
              {t("field.alerts")}
            </label>
            <select
              id="target-alerts"
              className="field"
              value={draft.alerts}
              onChange={(event) => patch("alerts", event.target.value as Draft["alerts"])}
            >
              <option value="inherit">{t("alerts.inherit")}</option>
              {ALERT_LEVELS.map((level) => (
                <option key={level} value={level}>
                  {t(`alerts.${level}`)}
                </option>
              ))}
            </select>
            <p className="hint">{t("field.alerts.hint")}</p>
          </div>

          <Toggle
            checked={draft.enabled}
            onChange={(value) => patch("enabled", value)}
            label={t("field.enabled")}
          />
        </div>

        <footer className="flex justify-end gap-2 border-t border-hairline px-5 py-4">
          <Button onClick={onClose}>{t("action.cancel")}</Button>
          <Button type="submit" variant="primary" disabled={saving}>
            {t("action.save")}
          </Button>
        </footer>
      </form>
    </div>
  );
}
