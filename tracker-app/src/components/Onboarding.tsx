import { useEffect, useState } from "react";
import { api, PluginInstall } from "../api";

export function Onboarding({
  onRefresh,
}: {
  onRefresh: () => Promise<void>;
}) {
  const [installing, setInstalling] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [pluginStatus, setPluginStatus] = useState<PluginInstall | null>(null);

  useEffect(() => {
    // Poll for the plugin while the user installs it from Claude Code.
    // When it lands we fire onRefresh so App.tsx re-evaluates and unmounts us.
    let cancelled = false;
    let lastSeen = false;
    const tick = async () => {
      try {
        const s = await api.getPluginStatus();
        if (cancelled) return;
        setPluginStatus(s);
        if (s && !lastSeen) {
          lastSeen = true;
          await onRefresh();
        }
      } catch {
        // transient errors are fine — we'll poll again
      }
    };
    tick();
    const id = setInterval(tick, 4000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [onRefresh]);

  const installCommand =
    "/plugin marketplace add Amoner/project-claude-tracker\n/plugin install claude-tracker";

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(installCommand);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setErr("clipboard access denied — copy manually");
    }
  };

  const handleLegacy = async () => {
    setInstalling(true);
    setErr(null);
    try {
      await api.installHooks();
      await api.runDiscover();
      await onRefresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setInstalling(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center p-8">
      <div className="max-w-xl space-y-4 rounded border border-zinc-800 bg-zinc-900 p-6 text-sm text-zinc-300">
        <h2 className="text-xl font-semibold text-zinc-100">
          Set up Claude Tracker
        </h2>
        <p>
          Claude Tracker is a Claude Code plugin that records sessions, prompts,
          and per-project metadata. Nothing leaves your machine.
        </p>

        <div className="space-y-2">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
            Install the plugin
          </h3>
          <p className="text-xs text-zinc-400">
            Run these two commands inside Claude Code:
          </p>
          <div className="relative">
            <pre className="overflow-x-auto rounded border border-zinc-700 bg-zinc-950 p-3 text-xs text-zinc-100">
              {installCommand}
            </pre>
            <button
              onClick={copy}
              className="absolute right-2 top-2 rounded border border-zinc-700 bg-zinc-900 px-2 py-0.5 text-[10px] text-zinc-300 hover:bg-zinc-800"
            >
              {copied ? "copied" : "copy"}
            </button>
          </div>
          {pluginStatus == null ? (
            <p className="text-xs text-zinc-500">
              Waiting for the plugin to show up… (this page refreshes automatically)
            </p>
          ) : (
            <p className="text-xs text-emerald-300">
              Plugin detected at{" "}
              <code className="rounded bg-zinc-800 px-1 py-0.5">
                {pluginStatus.path}
              </code>
            </p>
          )}
        </div>

        <details className="space-y-2 text-xs">
          <summary className="cursor-pointer text-zinc-400 hover:text-zinc-200">
            Prefer not to use plugins? Install hooks directly from this app.
          </summary>
          <div className="mt-2 space-y-2 rounded border border-zinc-800 bg-zinc-950/50 p-3">
            <p className="text-zinc-400">
              Writes the same five hooks into{" "}
              <code className="rounded bg-zinc-800 px-1 py-0.5">
                ~/.claude/settings.json
              </code>{" "}
              and backs the file up first. Works without a Claude Code plugin
              system — suitable as a fallback.
            </p>
            {err && (
              <div className="rounded border border-red-700 bg-red-900/30 p-2 text-red-300">
                {err}
              </div>
            )}
            <button
              onClick={handleLegacy}
              disabled={installing}
              className="rounded border border-zinc-700 px-3 py-1 text-zinc-200 hover:bg-zinc-800 disabled:opacity-50"
            >
              {installing ? "Installing…" : "Install hooks (legacy)"}
            </button>
          </div>
        </details>
      </div>
    </div>
  );
}
