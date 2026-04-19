import { useEffect, useState } from "react";
import { api, HookStatus, TerminalInfo } from "../api";

export function SettingsPage({
  hookStatus,
  onChanged,
}: {
  hookStatus: HookStatus | null;
  onChanged: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [terminals, setTerminals] = useState<TerminalInfo[]>([]);
  const [preferred, setPreferred] = useState<string | null>(null);
  const [terminalErr, setTerminalErr] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([api.listTerminals(), api.getPreferredTerminal()])
      .then(([ts, p]) => {
        setTerminals(ts);
        setPreferred(p);
      })
      .catch((e) => setTerminalErr(String(e)));
  }, []);

  const handleTerminalChange = async (slug: string) => {
    setTerminalErr(null);
    try {
      await api.setPreferredTerminal(slug);
      setPreferred(slug);
    } catch (e) {
      setTerminalErr(String(e));
    }
  };

  const activeSlug =
    preferred ?? terminals.find((t) => t.installed)?.slug ?? "";

  const wrap = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setErr(null);
    try {
      await fn();
      await onChanged();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!hookStatus) return null;
  return (
    <div className="flex-1 overflow-y-auto p-6 font-mono text-sm">
      <section className="mb-6 rounded border border-zinc-800 bg-zinc-900 p-4">
        <h2 className="mb-3 text-lg font-semibold text-zinc-100">Hooks</h2>
        <div className="grid grid-cols-[140px_1fr] gap-2 text-xs">
          <span className="text-zinc-500">settings.json</span>
          <span className="text-zinc-200">{hookStatus.settings_path}</span>
          <span className="text-zinc-500">installed events</span>
          <span className="text-zinc-200">
            {hookStatus.installed_events.length
              ? hookStatus.installed_events.join(", ")
              : "—"}
          </span>
          <span className="text-zinc-500">registered cli path</span>
          <span className="text-zinc-200 break-all">
            {hookStatus.cli_path ?? "—"}
          </span>
          <span className="text-zinc-500">status</span>
          <span
            className={
              hookStatus.fully_installed
                ? "text-emerald-300"
                : "text-amber-300"
            }
          >
            {hookStatus.fully_installed
              ? `all ${hookStatus.installed_events.length} events installed`
              : "incomplete"}
          </span>
        </div>
        <div className="mt-4 flex gap-2">
          <button
            onClick={() => wrap(() => api.installHooks())}
            disabled={busy}
            className="rounded border border-emerald-700 bg-emerald-900/40 px-3 py-1 text-xs text-emerald-200 hover:bg-emerald-900/60 disabled:opacity-50"
          >
            {hookStatus.fully_installed ? "Reinstall" : "Install"}
          </button>
          <button
            onClick={() => wrap(() => api.uninstallHooks())}
            disabled={busy}
            className="rounded border border-red-700 bg-red-900/30 px-3 py-1 text-xs text-red-200 hover:bg-red-900/50 disabled:opacity-50"
          >
            Uninstall
          </button>
          <button
            onClick={() => wrap(() => api.runDiscover())}
            disabled={busy}
            className="rounded border border-zinc-700 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-800 disabled:opacity-50"
          >
            Rescan ~/.claude/projects
          </button>
        </div>
        {err && (
          <div className="mt-3 rounded border border-red-700 bg-red-900/30 p-2 text-xs text-red-300">
            {err}
          </div>
        )}
      </section>

      <section className="mb-6 rounded border border-zinc-800 bg-zinc-900 p-4">
        <h2 className="mb-2 text-lg font-semibold text-zinc-100">Terminal</h2>
        <p className="mb-3 text-xs text-zinc-400">
          Used by the Start button to open a new window at the project folder and run{" "}
          <code className="rounded bg-zinc-800 px-1 py-0.5">claude</code>.
        </p>
        <div className="flex items-center gap-3">
          <span className="text-xs text-zinc-500">Preferred</span>
          <select
            value={activeSlug}
            onChange={(e) => handleTerminalChange(e.target.value)}
            disabled={terminals.length === 0}
            className="rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-xs disabled:opacity-50"
          >
            {terminals.map((t) => (
              <option key={t.slug} value={t.slug} disabled={!t.installed}>
                {t.display_name}
                {!t.installed ? " (not installed)" : ""}
              </option>
            ))}
          </select>
        </div>
        {terminalErr && (
          <div className="mt-3 rounded border border-red-700 bg-red-900/30 p-2 text-xs text-red-300">
            {terminalErr}
          </div>
        )}
      </section>

      <section className="rounded border border-zinc-800 bg-zinc-900 p-4 text-xs text-zinc-400">
        <h2 className="mb-2 text-lg font-semibold text-zinc-100">About</h2>
        <p>
          Tracker DB lives at{" "}
          <code className="rounded bg-zinc-800 px-1 py-0.5">
            ~/.claude-tracker/db.sqlite
          </code>
          . Logs at{" "}
          <code className="rounded bg-zinc-800 px-1 py-0.5">
            ~/.claude-tracker/logs/
          </code>
          .
        </p>
      </section>
    </div>
  );
}
