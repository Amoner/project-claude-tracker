import { Project } from "../api";

const PALETTE: Record<string, string> = {
  active: "bg-emerald-500/20 text-emerald-300 border-emerald-500/40",
  developing: "bg-sky-500/20 text-sky-300 border-sky-500/40",
  deployed: "bg-violet-500/20 text-violet-300 border-violet-500/40",
  planning: "bg-amber-500/20 text-amber-300 border-amber-500/40",
  idle: "bg-zinc-600/30 text-zinc-300 border-zinc-500/40",
  stale: "bg-red-500/15 text-red-300 border-red-500/30",
  archived: "bg-zinc-700/40 text-zinc-400 border-zinc-600",
};

export function StatusBadge({ project }: { project: Project }) {
  const status = project.effective_status;
  const cls = PALETTE[status] ?? PALETTE.idle;
  return (
    <span
      className={`inline-block rounded border px-1.5 py-0.5 text-[10px] uppercase tracking-wider ${cls}`}
    >
      {status}
    </span>
  );
}
