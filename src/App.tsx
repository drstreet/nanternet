import { useState } from "react";

import { AddServerModal } from "./components/AddServerModal";
import { Dashboard } from "./components/Dashboard";
import { Settings } from "./components/Settings";
import { I18nProvider, useI18n } from "./i18n";
import type { TargetView } from "./types";
import { useMonitor } from "./useMonitor";

type View = "dashboard" | "settings";

export function App() {
  const monitor = useMonitor();

  return (
    <I18nProvider language={monitor.settings?.language ?? "en"}>
      <Shell monitor={monitor} />
    </I18nProvider>
  );
}

function Shell({ monitor }: { monitor: ReturnType<typeof useMonitor> }) {
  const { t } = useI18n();
  const [view, setView] = useState<View>("dashboard");
  const [editing, setEditing] = useState<TargetView | null>(null);
  const [composing, setComposing] = useState(false);

  const remove = async (target: TargetView) => {
    if (window.confirm(t("action.confirmDelete", { name: target.name }))) {
      await monitor.remove(target.id);
    }
  };

  return (
    <div className="flex h-screen flex-col">
      <header className="flex shrink-0 items-center gap-4 border-b border-hairline px-5 py-3">
        <div className="min-w-0">
          <h1 className="truncate text-sm font-semibold text-ink">{t("app.name")}</h1>
          <p className="truncate text-xs text-ink-faint">{t("app.tagline")}</p>
        </div>

        <nav className="ms-auto flex gap-1 rounded-lg bg-surface-sunken p-1">
          {(["dashboard", "settings"] as const).map((entry) => (
            <button
              key={entry}
              type="button"
              onClick={() => setView(entry)}
              className={`rounded-md px-3 py-1.5 text-sm transition ${
                view === entry ? "bg-surface-raised text-ink" : "text-ink-muted hover:text-ink"
              }`}
            >
              {t(`nav.${entry}`)}
            </button>
          ))}
        </nav>
      </header>

      {monitor.error ? (
        <div className="shrink-0 border-b border-bad/40 bg-bad/10 px-5 py-2.5">
          <div className="flex items-start justify-between gap-4">
            <p className="font-mono text-xs leading-relaxed text-bad">{monitor.error}</p>
            <button
              type="button"
              onClick={monitor.dismissError}
              className="shrink-0 text-bad/70 transition hover:text-bad"
              aria-label={t("action.cancel")}
            >
              ✕
            </button>
          </div>
        </div>
      ) : null}

      <main className="min-h-0 flex-1 overflow-y-auto px-5 py-5">
        {!monitor.ready ? null : view === "dashboard" ? (
          <Dashboard
            targets={monitor.targets}
            statuses={monitor.statuses}
            checking={monitor.checking}
            onAdd={() => setComposing(true)}
            onEdit={setEditing}
            onDelete={remove}
            onToggle={(target) => void monitor.toggle(target.id, !target.enabled)}
            onCheck={(target) => void monitor.check(target.id)}
            onCheckAll={() => void monitor.checkAll()}
          />
        ) : monitor.settings ? (
          <Settings settings={monitor.settings} onSave={monitor.updateSettings} />
        ) : null}
      </main>

      {composing || editing ? (
        <AddServerModal
          target={editing}
          onClose={() => {
            setComposing(false);
            setEditing(null);
          }}
          onSubmit={monitor.save}
        />
      ) : null}
    </div>
  );
}
