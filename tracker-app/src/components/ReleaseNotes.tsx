type Props = {
  version: string;
  onClose: () => void;
};

export function ReleaseNotes({ version, onClose }: Props) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70">
      <div className="w-[520px] max-h-[80vh] overflow-y-auto rounded-lg border border-zinc-700 bg-zinc-900 shadow-2xl">
        <div className="flex items-center justify-between border-b border-zinc-800 px-5 py-4">
          <div>
            <h2 className="text-base font-semibold text-zinc-100">
              Welcome to Claude Tracker
            </h2>
            <p className="text-xs text-zinc-500 mt-0.5">v{version}</p>
          </div>
          <button
            onClick={onClose}
            className="rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
          >
            ✕
          </button>
        </div>

        <div className="space-y-5 px-5 py-5 text-sm text-zinc-300">
          <Section title="Quick setup">
            <Step n={1} label="Install hooks">
              Go to <strong className="text-zinc-100">Settings</strong> and click{" "}
              <strong className="text-zinc-100">Install Hooks</strong>. This wires
              Claude Code to log every session automatically — you only need to do
              this once.
            </Step>
            <Step n={2} label="Let it track">
              Every time you start a Claude Code session, the tracker records
              timing, prompt counts, and which project was active. No extra steps
              needed.
            </Step>
            <Step n={3} label="Launch Claude from here">
              Select any project and click{" "}
              <strong className="text-zinc-100">Start</strong> to open your
              preferred terminal at that project's directory with{" "}
              <code className="rounded bg-zinc-800 px-1 text-xs text-emerald-400">
                claude
              </code>{" "}
              already running.
            </Step>
          </Section>

          <Section title="Tips">
            <ul className="space-y-1.5 text-zinc-400">
              <li>
                <span className="text-zinc-200">Rescan</span> re-discovers new
                projects from your recent Claude Code sessions.
              </li>
              <li>
                <span className="text-zinc-200">Sync</span> refreshes GitHub and
                deploy metadata for all projects.
              </li>
              <li>
                Set your preferred terminal in{" "}
                <span className="text-zinc-200">Settings</span> — Ghostty,
                WezTerm, Alacritty, kitty, and Terminal.app are supported on Mac.
              </li>
            </ul>
          </Section>
        </div>

        <div className="flex justify-end border-t border-zinc-800 px-5 py-3">
          <button
            onClick={onClose}
            className="rounded bg-zinc-700 px-4 py-1.5 text-sm text-zinc-100 hover:bg-zinc-600"
          >
            Got it
          </button>
        </div>
      </div>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-zinc-500">
        {title}
      </h3>
      <div className="space-y-3">{children}</div>
    </div>
  );
}

function Step({
  n,
  label,
  children,
}: {
  n: number;
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex gap-3">
      <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-zinc-700 text-xs font-bold text-zinc-300">
        {n}
      </span>
      <div>
        <p className="font-medium text-zinc-200">{label}</p>
        <p className="mt-0.5 text-zinc-400">{children}</p>
      </div>
    </div>
  );
}
