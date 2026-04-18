import { Project } from "../api";
import { StatusBadge } from "./StatusBadge";

export function ProjectList({
  projects,
  selectedId,
  onSelect,
}: {
  projects: Project[];
  selectedId: number | null;
  onSelect: (id: number) => void;
}) {
  if (projects.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center p-4 text-sm text-zinc-500">
        no projects yet — start a Claude Code session anywhere and it will show up here.
      </div>
    );
  }
  return (
    <ul className="flex-1 overflow-y-auto">
      {projects.map((p) => {
        const active = p.id === selectedId;
        return (
          <li key={p.id}>
            <button
              onClick={() => onSelect(p.id)}
              className={`flex w-full flex-col gap-1 border-b border-zinc-800 px-3 py-2 text-left text-sm transition ${
                active
                  ? "bg-zinc-800"
                  : "hover:bg-zinc-800/50"
              }`}
            >
              <div className="flex items-center justify-between gap-2">
                <span className="truncate font-medium text-zinc-100">{p.name}</span>
                <StatusBadge project={p} />
              </div>
              <div className="flex items-center gap-2 text-[11px] text-zinc-500">
                <span>{relativeTime(p.last_active_at)}</span>
                <span>·</span>
                <span>{p.sessions_started}s</span>
                <span>{p.prompts_count}p</span>
              </div>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

function relativeTime(iso: string | null): string {
  if (!iso) return "never";
  const d = new Date(iso).getTime();
  const delta = Date.now() - d;
  const h = delta / 3_600_000;
  if (h < 1) return `${Math.max(1, Math.round(delta / 60_000))}m ago`;
  if (h < 24) return `${Math.round(h)}h ago`;
  const days = h / 24;
  if (days < 30) return `${Math.round(days)}d ago`;
  return new Date(iso).toLocaleDateString();
}
