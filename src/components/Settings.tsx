import { useEffect, useState } from "react";

import { sendTestNotification } from "../api";
import { useI18n } from "../i18n";
import type { AlertLevel, Language, Settings as SettingsShape, SettingsView } from "../types";
import { Button, Toggle, lines } from "./primitives";

export const ALERT_LEVELS: AlertLevel[] = ["all", "problems", "critical", "off"];

interface Props {
  settings: SettingsView;
  onSave: (settings: SettingsShape, telegramToken?: string) => Promise<boolean>;
}

export function Settings({ settings, onSave }: Props) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<SettingsShape>(settings);
  const [telegramToken, setTelegramToken] = useState("");

  const [resolvers, setResolvers] = useState(settings.dnsResolvers.join("\n"));
  const [chats, setChats] = useState(settings.telegramChatIds.join("\n"));
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setDraft(settings);
    setResolvers(settings.dnsResolvers.join("\n"));
    setChats(settings.telegramChatIds.join("\n"));
  }, [settings]);

  const patch = <K extends keyof SettingsShape>(key: K, value: SettingsShape[K]) => {
    setSaved(false);
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const submit = async () => {
    const next = {
      ...draft,
      dnsResolvers: lines(resolvers),
      telegramChatIds: lines(chats),
    };
    if (!(await onSave(next, telegramToken.length > 0 ? telegramToken : undefined))) return;
    setTelegramToken("");
    setSaved(true);
  };

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
      className="max-w-2xl space-y-5"
    >
      <section className="card space-y-4 px-5 py-5">
        <h2 className="text-sm font-medium text-ink">{t("settings.language")}</h2>
        <div className="grid gap-2 sm:grid-cols-2">
          {(
            [
              ["en", "English"],
              ["fa", "فارسی"],
            ] as const
          ).map(([code, label]) => (
            <button
              key={code}
              type="button"
              onClick={() => patch("language", code as Language)}
              className={`rounded-lg border px-3 py-2 text-sm transition ${
                draft.language === code
                  ? "border-brand bg-brand/10 text-ink"
                  : "border-hairline text-ink-muted hover:border-brand/50"
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      </section>

      <section className="card space-y-4 px-5 py-5">
        <h2 className="text-sm font-medium text-ink">{t("settings.notifications")}</h2>

        <Toggle
          checked={draft.desktopNotifications}
          onChange={(value) => patch("desktopNotifications", value)}
          label={t("settings.desktop")}
        />

        <div>
          <label className="label" htmlFor="settings-discord">
            {t("settings.discord")}
          </label>
          <input
            id="settings-discord"
            type="url"
            className="field font-mono"
            placeholder="https://discord.com/api/webhooks/…"
            value={draft.discordWebhook ?? ""}
            onChange={(event) => patch("discordWebhook", event.target.value || null)}
          />
        </div>

        <div>
          <label className="label" htmlFor="settings-alerts">
            {t("settings.alerts")}
          </label>
          <select
            id="settings-alerts"
            className="field"
            value={draft.alerts}
            onChange={(event) => patch("alerts", event.target.value as AlertLevel)}
          >
            {ALERT_LEVELS.map((level) => (
              <option key={level} value={level}>
                {t(`alerts.${level}`)}
              </option>
            ))}
          </select>
          <p className="hint">{t("settings.alerts.hint")}</p>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <label className="label" htmlFor="settings-telegram-chat">
              {t("settings.telegramChat")}
            </label>
            <textarea
              id="settings-telegram-chat"
              rows={3}
              className="field font-mono"
              placeholder="-1001234567890&#10;@mychannel&#10;123456789"
              value={chats}
              onChange={(event) => {
                setSaved(false);
                setChats(event.target.value);
              }}
            />
            <p className="hint">{t("settings.telegramChat.hint")}</p>
          </div>
          <div>
            <label className="label" htmlFor="settings-telegram-token">
              {t("settings.telegramToken")}
            </label>
            <input
              id="settings-telegram-token"
              type="password"
              className="field font-mono"
              autoComplete="off"
              value={telegramToken}
              onChange={(event) => setTelegramToken(event.target.value)}
            />
            {settings.telegramTokenStored ? (
              <p className="hint">
                {`${t("settings.telegramToken.stored")} ${t("auth.keepStored")}`}
              </p>
            ) : null}
          </div>
        </div>

        <div>
          <label className="label" htmlFor="settings-proxy">
            {t("settings.proxy")}
          </label>
          <input
            id="settings-proxy"
            className="field font-mono"
            placeholder="socks5h://127.0.0.1:10808"
            value={draft.proxyUrl ?? ""}
            onChange={(event) => patch("proxyUrl", event.target.value || null)}
          />
          <p className="hint">{t("settings.proxy.hint")}</p>
        </div>

        <Button onClick={() => void sendTestNotification()}>{t("action.test")}</Button>
      </section>

      <section className="card space-y-4 px-5 py-5">
        <h2 className="text-sm font-medium text-ink">{t("settings.behaviour")}</h2>

        <div>
          <label className="label" htmlFor="settings-confirmations">
            {t("settings.confirmations")}
          </label>
          <select
            id="settings-confirmations"
            className="field"
            value={draft.confirmations}
            onChange={(event) => patch("confirmations", Number(event.target.value))}
          >
            {[1, 2, 3, 4, 5].map((count) => (
              <option key={count} value={count}>
                {count}
              </option>
            ))}
          </select>
          <p className="hint">{t("settings.confirmations.hint")}</p>
        </div>

        <div>
          <label className="label" htmlFor="settings-resolvers">
            {t("settings.dnsResolvers")}
          </label>
          <textarea
            id="settings-resolvers"
            rows={3}
            className="field font-mono"
            placeholder="10.202.10.102&#10;9.9.9.9"
            value={resolvers}
            onChange={(event) => {
              setSaved(false);
              setResolvers(event.target.value);
            }}
          />
          <p className="hint">{t("settings.dnsResolvers.hint")}</p>
        </div>

        <Toggle
          checked={draft.startAtLogin}
          onChange={(value) => patch("startAtLogin", value)}
          label={t("settings.startAtLogin")}
          hint={t("settings.background")}
        />
      </section>

      <section className="card px-5 py-5">
        <h2 className="text-sm font-medium text-ink">{t("settings.storage")}</h2>
        <p className="hint">{t("settings.storage.hint")}</p>
      </section>

      <div className="flex items-center gap-3">
        <Button type="submit" variant="primary">
          {t("action.save")}
        </Button>
        {saved ? <span className="text-xs text-good">{t("settings.saved")}</span> : null}
      </div>
    </form>
  );
}
