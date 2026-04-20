import { useEffect, useState } from "react";
import { api, HookStatus, PluginInstall, TerminalInfo } from "../api";

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
  const [pluginStatus, setPluginStatus] = useState<PluginInstall | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    Promise.all([api.listTerminals(), api.getPreferredTerminal()])
      .then(([ts, p]) => {
        setTerminals(ts);
        setPreferred(p);
      })
      .catch((e) => setTerminalErr(String(e)));
  }, []);

  useEffect(() => {
    api.getPluginStatus().then(setPluginStatus).catch(() => setPluginStatus(null));
  }, [hookStatus]);

  const refreshPlugin = async () => {
    try {
      const s = await api.getPluginStatus();
      setPluginStatus(s);
    } catch {
      setPluginStatus(null);
    }
  };

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

  const installCommand =
    "/plugin marketplace add Amoner/project-claude-tracker\n/plugin install claude-tracker";

  const copyInstall = async () => {
    try {
      await navigator.clipboard.writeText(installCommand);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // best effort
    }
  };

  if (!hookStatus) return null;
  return (
    <div className="flex-1 overflow-y-auto p-6 font-mono text-sm">
      <section className="mb-6 rounded border border-zinc-800 bg-zinc-900 p-4">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-lg font-semibold text-zinc-100">Plugin</h2>
          <button
            onClick={refreshPlugin}
            className="rounded border border-zinc-700 px-2 py-0.5 text-xs text-zinc-300 hover:bg-zinc-800"
          >
            Refresh
          </button>
        </div>
        {pluginStatus ? (
          <div className="grid grid-cols-[140px_1fr] gap-2 text-xs">
            <span className="text-zinc-500">status</span>
            <span className="text-emerald-300">installed ✓</span>
            <span className="text-zinc-500">version</span>
            <span className="text-zinc-200">{pluginStatus.version ?? "—"}</span>
            <span className="text-zinc-500">path</span>
            <span className="break-all text-zinc-200">{pluginStatus.path}</span>
          </div>
        ) : (
          <div className="space-y-2 text-xs">
            <p className="text-zinc-400">
              Not installed. Run these two commands inside Claude Code:
            </p>
            <div className="relative">
              <pre className="overflow-x-auto rounded border border-zinc-700 bg-zinc-950 p-3 text-xs text-zinc-100">
                {installCommand}
              </pre>
              <button
                onClick={copyInstall}
                className="absolute right-2 top-2 rounded border border-zinc-700 bg-zinc-900 px-2 py-0.5 text-[10px] text-zinc-300 hover:bg-zinc-800"
              >
                {copied ? "copied" : "copy"}
              </button>
            </div>
          </div>
        )}
      </section>

      <details className="mb-6 rounded border border-zinc-800 bg-zinc-900">
        <summary className="cursor-pointer px-4 py-3 text-sm text-zinc-300 hover:text-zinc-100">
          Legacy: install hooks directly into settings.json
        </summary>
        <div className="space-y-3 border-t border-zinc-800 px-4 py-3">
          <p className="text-xs text-zinc-500">
            Fallback for users not running the plugin. Writes the five tracker
            hooks into{" "}
            <code className="rounded bg-zinc-800 px-1 py-0.5">
              ~/.claude/settings.json
            </code>{" "}
            directly, backing up first. Safe to leave installed alongside the
            plugin — duplicate hook commands dedupe.
          </p>
          <div className="grid grid-cols-[140px_1fr] gap-2 text-xs">
            <span className="text-zinc-500">settings.json</span>
            <span className="break-all text-zinc-200">
              {hookStatus.settings_path}
            </span>
            <span className="text-zinc-500">installed events</span>
            <span className="text-zinc-200">
              {hookStatus.installed_events.length
                ? hookStatus.installed_events.join(", ")
                : "—"}
            </span>
            <span className="text-zinc-500">registered cli path</span>
            <span className="break-all text-zinc-200">
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
          <div className="flex gap-2">
            <button
              onClick={() => wrap(() => api.installHooks())}
              disabled={busy}
              className="rounded border border-zinc-700 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-800 disabled:opacity-50"
            >
              {hookStatus.fully_installed ? "Reinstall" : "Install"}
            </button>
            <button
              onClick={() => wrap(() => api.uninstallHooks())}
              disabled={busy}
              className="rounded border border-zinc-700 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-800 disabled:opacity-50"
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
            <div className="rounded border border-red-700 bg-red-900/30 p-2 text-xs text-red-300">
              {err}
            </div>
          )}
        </div>
      </details>

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
