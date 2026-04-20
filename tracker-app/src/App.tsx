import { useCallback, useEffect, useMemo, useState } from "react";
import { api, HookStatus, PluginInstall, Project } from "./api";
import { ProjectList } from "./components/ProjectList";
import { ProjectDetail } from "./components/ProjectDetail";
import { Onboarding } from "./components/Onboarding";
import { SettingsPage } from "./components/SettingsPage";
import { ReleaseNotes } from "./components/ReleaseNotes";
import { FindProjects } from "./components/FindProjects";

type Tab = "dashboard" | "settings";

export default function App() {
  const [tab, setTab] = useState<Tab>("dashboard");
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [hookStatus, setHookStatus] = useState<HookStatus | null>(null);
  const [pluginStatus, setPluginStatus] = useState<PluginInstall | null>(null);
  const [loading, setLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [releaseVersion, setReleaseVersion] = useState<string | null>(null);
  const [findOpen, setFindOpen] = useState(false);

  const refresh = useCallback(async () => {
    const [ps, hs, plug] = await Promise.all([
      api.listProjects(false),
      api.getHookStatus(),
      api.getPluginStatus().catch(() => null),
    ]);
    setProjects(ps);
    setHookStatus(hs);
    setPluginStatus(plug);
    setLoading(false);
    if (selectedId == null && ps.length) setSelectedId(ps[0].id);
  }, [selectedId]);

  useEffect(() => {
    refresh().catch((e) => {
      console.error(e);
      setLoading(false);
    });
  }, [refresh]);

  useEffect(() => {
    api.checkReleaseNotes().then(setReleaseVersion).catch(console.error);
  }, []);

  const selected = useMemo(
    () => projects.find((p) => p.id === selectedId) ?? null,
    [projects, selectedId],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return projects;
    return projects.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.path.toLowerCase().includes(q) ||
        (p.github_url ?? "").toLowerCase().includes(q),
    );
  }, [projects, query]);

  const handleSync = async () => {
    setSyncing(true);
    try {
      await api.runSync(true, true);
      await refresh();
    } finally {
      setSyncing(false);
    }
  };

  const handleDiscover = async () => {
    setSyncing(true);
    try {
      await api.runDiscover();
      await refresh();
    } finally {
      setSyncing(false);
    }
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-zinc-500">
        loading…
      </div>
    );
  }

  const needsOnboarding =
    hookStatus != null &&
    !hookStatus.fully_installed &&
    pluginStatus == null &&
    projects.length === 0;

  if (needsOnboarding) {
    return <Onboarding onRefresh={refresh} />;
  }

  return (
    <div className="flex h-full flex-col">
      {releaseVersion && (
        <ReleaseNotes
          version={releaseVersion}
          onClose={() => setReleaseVersion(null)}
        />
      )}
      {findOpen && (
        <FindProjects
          onClose={() => setFindOpen(false)}
          onImported={refresh}
        />
      )}
      <header className="flex items-center justify-between border-b border-zinc-800 bg-zinc-900 px-4 py-2">
        <div className="flex items-center gap-6">
          <h1 className="text-sm font-semibold uppercase tracking-wider text-zinc-200">
            Claude Tracker
          </h1>
          <nav className="flex gap-1 text-xs">
            <TabButton active={tab === "dashboard"} onClick={() => setTab("dashboard")}>
              Dashboard
            </TabButton>
            <TabButton active={tab === "settings"} onClick={() => setTab("settings")}>
              Settings
            </TabButton>
          </nav>
        </div>
        <div className="flex items-center gap-2 text-xs text-zinc-400">
          {hookStatus && (
            <span
              className={
                hookStatus.fully_installed
                  ? "text-emerald-400"
                  : "text-amber-400"
              }
            >
              {hookStatus.fully_installed ? "hooks ok" : "hooks not installed"}
            </span>
          )}
          <button
            onClick={() => setFindOpen(true)}
            className="rounded border border-zinc-700 px-2 py-1 text-zinc-300 hover:bg-zinc-800"
          >
            Find
          </button>
          <button
            onClick={handleDiscover}
            disabled={syncing}
            className="rounded border border-zinc-700 px-2 py-1 text-zinc-300 hover:bg-zinc-800 disabled:opacity-50"
          >
            Rescan
          </button>
          <button
            onClick={handleSync}
            disabled={syncing}
            className="rounded border border-zinc-700 px-2 py-1 text-zinc-300 hover:bg-zinc-800 disabled:opacity-50"
          >
            {syncing ? "syncing…" : "Sync"}
          </button>
        </div>
      </header>

      {tab === "dashboard" ? (
        <div className="flex flex-1 overflow-hidden">
          <aside className="flex w-80 flex-col border-r border-zinc-800 bg-zinc-900">
            <div className="border-b border-zinc-800 p-2">
              <input
                placeholder="Search…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                className="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1 text-sm outline-none placeholder:text-zinc-500 focus:border-zinc-500"
              />
            </div>
            <ProjectList
              projects={filtered}
              selectedId={selectedId}
              onSelect={setSelectedId}
            />
          </aside>
          <main className="flex-1 overflow-y-auto">
            {selected ? (
              <ProjectDetail
                project={selected}
                onChange={async (fields) => {
                  await api.updateProject(selected.id, fields);
                  await refresh();
                }}
                onResync={async () => {
                  await api.runSync(true, true);
                  await refresh();
                }}
              />
            ) : (
              <div className="flex h-full items-center justify-center text-zinc-500">
                no project selected
              </div>
            )}
          </main>
        </div>
      ) : (
        <SettingsPage
          hookStatus={hookStatus}
          onChanged={refresh}
        />
      )}
    </div>
  );
}

function TabButton({
  active,
  children,
  onClick,
}: {
  active: boolean;
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`rounded px-2 py-1 ${
        active
          ? "bg-zinc-800 text-zinc-100"
          : "text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
      }`}
    >
      {children}
    </button>
  );
}
