import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { api, on } from "./api";
import type { Card, Progress, Report, Settings } from "./types";
import Home from "./screens/Home";
import Cull from "./screens/Cull";
import ImportDialog from "./components/ImportDialog";
import ReportDialog from "./components/ReportDialog";
import SettingsDialog from "./components/SettingsDialog";
import { Toast, ToastMsg } from "./components/Toast";

export type ImportSource = { source: string; label: string; cardMount?: string; auto?: boolean };

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [cards, setCards] = useState<Card[]>([]);
  const [projectRoot, setProjectRoot] = useState<string | null>(null);
  const [importSource, setImportSource] = useState<ImportSource | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [report, setReport] = useState<Report | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [toast, setToast] = useState<ToastMsg | null>(null);
  const [homeKey, setHomeKey] = useState(0);
  // Cards the user dismissed (or already handled) this session, so the popup doesn't nag.
  const [seenCards, setSeenCards] = useState<Set<string>>(new Set());

  const notify = useCallback((t: ToastMsg) => setToast(t), []);

  useEffect(() => {
    api.getSettings().then(setSettings);
    api.listCards().then(setCards);
    api.importStatus().then((p) => p && setProgress(p));
    const subs = [
      on("cards-changed", setCards),
      on("import-progress", (p) => setProgress(p.finished ? null : p)),
      on("import-finished", (r) => {
        setProgress(null);
        setReport(r);
        setHomeKey((k) => k + 1);
      }),
      on("import-error", (text) => {
        setProgress(null);
        setToast({ text, bad: true });
        setHomeKey((k) => k + 1);
      }),
    ];
    return () => void subs.forEach((s) => s.then((u) => u()));
  }, []);

  // A newly inserted card opens the import popup straight away.
  useEffect(() => {
    const fresh = cards.find((c) => !seenCards.has(c.mount));
    if (fresh && !importSource && !progress) {
      setSeenCards((s) => new Set(s).add(fresh.mount));
      setImportSource({ source: fresh.mount, label: fresh.label, cardMount: fresh.mount, auto: true });
    }
    // Forget cards that were removed, so re-inserting one prompts again.
    setSeenCards((s) => {
      const still = new Set([...s].filter((m) => cards.some((c) => c.mount === m)));
      return still.size === s.size ? s : still;
    });
  }, [cards]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!settings) return null;

  return (
    <>
      {projectRoot ? (
        <Cull
          root={projectRoot}
          progress={progress?.projectRoot === projectRoot ? progress : null}
          onBack={() => {
            setProjectRoot(null);
            setHomeKey((k) => k + 1);
          }}
          notify={notify}
        />
      ) : (
        <Home
          key={homeKey}
          cards={cards}
          progress={progress}
          onImport={setImportSource}
          onOpen={setProjectRoot}
          onSettings={() => setShowSettings(true)}
          notify={notify}
        />
      )}

      {importSource && (
        <ImportDialog
          source={importSource}
          settings={settings}
          onClose={() => setImportSource(null)}
          onNothingNew={importSource.auto ? (label) => {
            setImportSource(null);
            notify({ text: `Nothing new on ${label}. Everything is already imported.` });
          } : undefined}
          onStarted={(root) => {
            setImportSource(null);
            setProjectRoot(root);
          }}
        />
      )}
      {report && (
        <ReportDialog
          report={report}
          onClose={() => setReport(null)}
          onOpen={() => {
            setProjectRoot(report.projectRoot);
            setReport(null);
          }}
          notify={notify}
        />
      )}
      {showSettings && (
        <SettingsDialog
          settings={settings}
          onClose={() => setShowSettings(false)}
          onSaved={(s) => {
            setSettings(s);
            setShowSettings(false);
            setHomeKey((k) => k + 1);
          }}
          notify={notify}
        />
      )}
      {toast && <Toast msg={toast} onDone={() => setToast(null)} />}
    </>
  );
}
