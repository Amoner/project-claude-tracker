import { useState } from "react";

export function Onboarding({
  onInstalled,
}: {
  onInstalled: () => Promise<void>;
}) {
  const [installing, setInstalling] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const handle = async () => {
    setInstalling(true);
    setErr(null);
    try {
      await onInstalled();
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
          This app tracks every Claude Code project you work on: where it lives,
          its GitHub URL, deploy URL, launch/deploy instructions, and live
          activity. Nothing leaves your machine.
        </p>
        <p>
          We need to install five global hooks in{" "}
          <code className="rounded bg-zinc-800 px-1 py-0.5 text-xs">
            ~/.claude/settings.json
          </code>{" "}
          — SessionStart, SessionEnd, UserPromptSubmit, Stop, and CwdChanged.
          Your existing hooks (like the Glass notification sound) are preserved;
          a backup is written to <code className="rounded bg-zinc-800 px-1 py-0.5 text-xs">~/.claude/backups/</code>.
        </p>
        <p>
          After install we'll scan <code className="rounded bg-zinc-800 px-1 py-0.5 text-xs">~/.claude/projects/</code> to
          seed every project you've ever opened in Claude Code.
        </p>
        {err && (
          <div className="rounded border border-red-700 bg-red-900/30 p-2 text-xs text-red-300">
            {err}
          </div>
        )}
        <div className="pt-2">
          <button
            onClick={handle}
            disabled={installing}
            className="rounded bg-emerald-600 px-4 py-2 text-sm font-semibold text-white hover:bg-emerald-500 disabled:opacity-50"
          >
            {installing ? "Installing…" : "Install hooks & scan"}
          </button>
        </div>
      </div>
    </div>
  );
}
