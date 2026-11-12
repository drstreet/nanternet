import { useEffect, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

import { errorText } from "../api";
import { useI18n } from "../i18n";
import { Button } from "./primitives";

const RECHECK_MS = 6 * 60 * 60 * 1000;

export function UpdateBanner() {
  const { t } = useI18n();
  const [update, setUpdate] = useState<Update | null>(null);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    const look = () =>
      check()
        .then(setUpdate)
        .catch(() => undefined);
    look();
    const timer = window.setInterval(look, RECHECK_MS);
    return () => window.clearInterval(timer);
  }, []);

  if (!update || dismissed === update.version) return null;

  const install = async () => {
    setInstalling(true);
    setFailure(null);
    try {
      await update.downloadAndInstall();
      await relaunch();
    } catch (cause) {
      setFailure(errorText(cause));
      setInstalling(false);
    }
  };

  return (
    <div className="shrink-0 border-b border-brand/40 bg-brand/10 px-5 py-2.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm text-ink">
          {failure
            ? t("update.failed", { reason: failure })
            : installing
              ? t("update.installing")
              : t("update.available", { version: update.version })}
        </p>
        <div className="flex gap-2">
          <Button onClick={() => setDismissed(update.version)} disabled={installing}>
            {t("update.later")}
          </Button>
          <Button variant="primary" onClick={() => void install()} disabled={installing}>
            {t("update.install")}
          </Button>
        </div>
      </div>
    </div>
  );
}
