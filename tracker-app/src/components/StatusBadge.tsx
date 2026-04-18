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
  const status = effectiveStatus(project);
  const cls = PALETTE[status] ?? PALETTE.idle;
  return (
    <span
      className={`inline-block rounded border px-1.5 py-0.5 text-[10px] uppercase tracking-wider ${cls}`}
    >
      {status}
    </span>
  );
}

export function effectiveStatus(p: Project): string {
  if (p.status_manual && p.status) return p.status;
  if (p.archived_at) return "archived";
  if (p.deploy_url && p.deploy_url.trim()) return "deployed";
  if (!p.last_active_at) return "stale";
  const last = new Date(p.last_active_at).getTime();
  const daysAgo = (Date.now() - last) / 86_400_000;
  if (daysAgo < 7) return "active";
  if (daysAgo < 30) return "idle";
  return "stale";
}
