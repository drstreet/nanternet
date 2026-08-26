import { useCallback, useEffect, useState } from "react";

import * as api from "./api";
import type { Settings, SettingsView, Target, TargetStatus, TargetView } from "./types";

const FAILED = Symbol("failed");

export function useMonitor() {
  const [targets, setTargets] = useState<TargetView[]>([]);
  const [statuses, setStatuses] = useState<Record<string, TargetStatus>>({});
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [checking, setChecking] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

  const attempt = useCallback(async <T,>(work: () => Promise<T>) => {
    try {
      const outcome = await work();
      setError(null);
      return outcome;
    } catch (cause) {
      setError(api.errorText(cause));
      return FAILED;
    }
  }, []);

  const reload = useCallback(async () => {
    const loaded = await attempt(() =>
      Promise.all([api.listTargets(), api.listStatuses(), api.getSettings()]),
    );
    if (loaded !== FAILED) {
      const [loadedTargets, loadedStatuses, loadedSettings] = loaded;
      setTargets(loadedTargets);
      setStatuses(Object.fromEntries(loadedStatuses.map((status) => [status.targetId, status])));
      setSettings(loadedSettings);
    }
    setReady(true);
  }, [attempt]);

  useEffect(() => {
    void reload();
    const subscription = api.onStatus((status) =>
      setStatuses((current) => ({ ...current, [status.targetId]: status })),
    );
    return () => {
      void subscription.then((unlisten) => unlisten());
    };
  }, [reload]);

  const save = useCallback(
    async (target: Target, secret?: string) => {
      const saved = await attempt(() => api.saveTarget(target, secret));
      if (saved === FAILED) return false;
      await reload();
      return true;
    },
    [attempt, reload],
  );

  const remove = useCallback(
    async (id: string) => {
      if ((await attempt(() => api.deleteTarget(id))) === FAILED) return;
      setStatuses((current) => {
        const next = { ...current };
        delete next[id];
        return next;
      });
      await reload();
    },
    [attempt, reload],
  );

  const toggle = useCallback(
    async (id: string, enabled: boolean) => {
      setTargets((current) =>
        current.map((target) => (target.id === id ? { ...target, enabled } : target)),
      );
      if ((await attempt(() => api.setTargetEnabled(id, enabled))) === FAILED) await reload();
    },
    [attempt, reload],
  );

  const check = useCallback(
    async (id: string) => {
      setChecking((current) => [...current, id]);
      const status = await attempt(() => api.checkNow(id));
      setChecking((current) => current.filter((entry) => entry !== id));
      if (status !== FAILED) setStatuses((current) => ({ ...current, [id]: status }));
    },
    [attempt],
  );

  const checkAll = useCallback(async () => {
    await Promise.all(
      targets.filter((target) => target.enabled).map((target) => check(target.id)),
    );
  }, [check, targets]);

  const updateSettings = useCallback(
    async (next: Settings, telegramToken?: string) => {
      if ((await attempt(() => api.saveSettings(next, telegramToken))) === FAILED) return false;
      await reload();
      return true;
    },
    [attempt, reload],
  );

  return {
    targets,
    statuses,
    settings,
    checking,
    error,
    ready,
    dismissError: () => setError(null),
    save,
    remove,
    toggle,
    check,
    checkAll,
    updateSettings,
  };
}
