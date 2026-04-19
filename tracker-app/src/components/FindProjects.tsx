import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, ScanCandidate } from "../api";

type Tab = "ide" | "fs" | "manual";

const DEFAULT_ROOTS = [
  "~/Documents",
  "~/Projects",
  "~/src",
  "~/dev",
  "~/code",
  "~/workspace",
  "~/git",
  "~/repos",
];

const MAX_DEPTH = 5;

export function FindProjects({
  onClose,
  onImported,
}: {
  onClose: () => void;
  onImported: () => Promise<void>;
}) {
  const [tab, setTab] = useState<Tab>("ide");
  const [ide, setIde] = useState<ScanCandidate[] | null>(null);
  const [fs, setFs] = useState<ScanCandidate[] | null>(null);
  const [fsRoots, setFsRoots] = useState(DEFAULT_ROOTS.join("\n"));
  const [fsScanning, setFsScanning] = useState(false);
  const [manual, setManual] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    api.scanIdeProjects().then(setIde).catch((e) => setErr(String(e)));
  }, []);

  const activeList = tab === "ide" ? ide : tab === "fs" ? fs : null;

  const runFsScan = async () => {
    setFsScanning(true);
    setErr(null);
    try {
      const roots = fsRoots
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean);
      const result = await api.scanFilesystem(roots, MAX_DEPTH);
      setFs(result);
      // Drop any previously-selected fs paths that are no longer in the list.
      setSelected((prev) => {
        const available = new Set(result.map((c) => c.path));
        const next = new Set<string>();
        for (const p of prev) if (available.has(p)) next.add(p);
        return next;
      });
    } catch (e) {
      setErr(String(e));
    } finally {
      setFsScanning(false);
    }
  };

  const toggle = (path: string, disabled: boolean) => {
    if (disabled) return;
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const toggleAll = (list: ScanCandidate[]) => {
    const eligible = list.filter((c) => !c.already_tracked).map((c) => c.path);
    const allSelected = eligible.every((p) => selected.has(p));
    setSelected((prev) => {
      const next = new Set(prev);
      if (allSelected) {
        for (const p of eligible) next.delete(p);
      } else {
        for (const p of eligible) next.add(p);
      }
      return next;
    });
  };

  const importSelected = async () => {
    if (selected.size === 0) return;
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      const added = await api.importProjects([...selected]);
      setMsg(`imported ${added} project${added === 1 ? "" : "s"}`);
      setSelected(new Set());
      await onImported();
      // Refresh the current list so already_tracked flips.
      if (tab === "ide") setIde(await api.scanIdeProjects());
      else if (tab === "fs") await runFsScan();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleBrowse = async () => {
    setErr(null);
    try {
      const picked = await open({ directory: true, multiple: false });
      if (typeof picked === "string") setManual(picked);
    } catch (e) {
      setErr(String(e));
    }
  };

  const handleManualAdd = async () => {
    const path = manual.trim();
    if (!path) return;
    setBusy(true);
    setErr(null);
    setMsg(null);
    try {
      const project = await api.addProjectManual(path);
      setMsg(`added ${project.name}`);
      setManual("");
      await onImported();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70">
      <div className="flex h-[70vh] w-[640px] flex-col rounded-lg border border-zinc-700 bg-zinc-900 shadow-2xl">
        <div className="flex items-center justify-between border-b border-zinc-800 px-5 py-4">
          <h2 className="text-base font-semibold text-zinc-100">Find projects</h2>
          <button
            onClick={onClose}
            className="rounded p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200"
          >
            ✕
          </button>
        </div>

        <nav className="flex gap-1 border-b border-zinc-800 px-5 py-2 text-xs">
          <TabButton active={tab === "ide"} onClick={() => setTab("ide")}>
            IDE history
          </TabButton>
          <TabButton active={tab === "fs"} onClick={() => setTab("fs")}>
            Filesystem
          </TabButton>
          <TabButton active={tab === "manual"} onClick={() => setTab("manual")}>
            Manual
          </TabButton>
        </nav>

        <div className="flex-1 overflow-y-auto px-5 py-4 text-sm">
          {tab === "ide" && (
            <CandidateList
              list={ide}
              selected={selected}
              onToggle={toggle}
              onToggleAll={toggleAll}
              emptyHint="No recent projects found in VS Code, Cursor, or JetBrains caches."
              loadingHint="scanning IDE caches…"
            />
          )}
          {tab === "fs" && (
            <div className="flex flex-col gap-3">
              <label className="flex flex-col gap-1 text-xs">
                <span className="text-zinc-400">
                  Dev roots (one per line, `~` expanded). Depth {MAX_DEPTH}, `.git`
                  directories mark projects.
                </span>
                <textarea
                  value={fsRoots}
                  onChange={(e) => setFsRoots(e.target.value)}
                  rows={5}
                  className="rounded border border-zinc-700 bg-zinc-950 px-2 py-1 font-mono text-xs"
                />
              </label>
              <div>
                <button
                  onClick={runFsScan}
                  disabled={fsScanning}
                  className="rounded border border-zinc-700 bg-zinc-800 px-3 py-1 text-xs text-zinc-100 hover:bg-zinc-700 disabled:opacity-50"
                >
                  {fsScanning ? "scanning…" : fs == null ? "Scan" : "Re-scan"}
                </button>
              </div>
              <CandidateList
                list={fs}
                selected={selected}
                onToggle={toggle}
                onToggleAll={toggleAll}
                emptyHint={
                  fs == null ? null : "No git repos found in those roots."
                }
                loadingHint={fsScanning ? "walking filesystem…" : null}
              />
            </div>
          )}
          {tab === "manual" && (
            <div className="flex flex-col gap-3">
              <p className="text-xs text-zinc-400">
                Add a project by picking its folder or typing its absolute
                path. It doesn't need a `.git` directory.
              </p>
              <div className="flex gap-2">
                <input
                  value={manual}
                  onChange={(e) => setManual(e.target.value)}
                  placeholder="/path/to/project or ~/path/to/project"
                  className="flex-1 rounded border border-zinc-700 bg-zinc-950 px-2 py-1 font-mono text-xs"
                />
                <button
                  onClick={handleBrowse}
                  className="rounded border border-zinc-700 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-800"
                >
                  Browse…
                </button>
                <button
                  onClick={handleManualAdd}
                  disabled={busy || !manual.trim()}
                  className="rounded bg-emerald-600/80 px-3 py-1 text-xs font-semibold text-white hover:bg-emerald-600 disabled:opacity-50"
                >
                  Add
                </button>
              </div>
            </div>
          )}
        </div>

        {(err || msg) && (
          <div className="border-t border-zinc-800 px-5 py-2 text-xs">
            {err && (
              <span className="text-red-300">{err}</span>
            )}
            {msg && !err && <span className="text-emerald-300">{msg}</span>}
          </div>
        )}

        <div className="flex items-center justify-between border-t border-zinc-800 px-5 py-3">
          <span className="text-xs text-zinc-500">
            {tab !== "manual" && activeList
              ? `${selected.size} of ${activeList.filter((c) => !c.already_tracked).length} selectable`
              : ""}
          </span>
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="rounded border border-zinc-700 px-3 py-1 text-xs text-zinc-300 hover:bg-zinc-800"
            >
              Close
            </button>
            {tab !== "manual" && (
              <button
                onClick={importSelected}
                disabled={busy || selected.size === 0}
                className="rounded bg-emerald-600/80 px-3 py-1 text-xs font-semibold text-white hover:bg-emerald-600 disabled:opacity-50"
              >
                {busy ? "importing…" : `Import selected`}
              </button>
            )}
          </div>
        </div>
      </div>
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

function CandidateList({
  list,
  selected,
  onToggle,
  onToggleAll,
  emptyHint,
  loadingHint,
}: {
  list: ScanCandidate[] | null;
  selected: Set<string>;
  onToggle: (path: string, disabled: boolean) => void;
  onToggleAll: (list: ScanCandidate[]) => void;
  emptyHint: string | null;
  loadingHint: string | null;
}) {
  if (list == null) {
    return (
      <p className="text-xs text-zinc-500">{loadingHint ?? "not scanned yet"}</p>
    );
  }
  if (list.length === 0) {
    return <p className="text-xs text-zinc-500">{emptyHint ?? "no results"}</p>;
  }
  const hasSelectable = list.some((c) => !c.already_tracked);
  return (
    <div className="flex flex-col gap-1">
      {hasSelectable && (
        <button
          onClick={() => onToggleAll(list)}
          className="self-start text-xs text-zinc-400 underline underline-offset-2 hover:text-zinc-200"
        >
          Toggle all
        </button>
      )}
      <ul className="flex flex-col gap-1">
        {list.map((c) => {
          const checked = selected.has(c.path);
          return (
            <li
              key={c.path}
              className="flex items-center gap-2 rounded border border-zinc-800 bg-zinc-950 px-2 py-1 text-xs"
            >
              <input
                type="checkbox"
                checked={checked}
                disabled={c.already_tracked}
                onChange={() => onToggle(c.path, c.already_tracked)}
              />
              <div className="flex-1 overflow-hidden">
                <div className="truncate font-medium text-zinc-100">{c.name}</div>
                <div className="truncate font-mono text-[10px] text-zinc-500">
                  {c.path}
                </div>
              </div>
              <span className="rounded border border-zinc-700 px-1 text-[10px] uppercase tracking-wider text-zinc-400">
                {c.source}
              </span>
              {c.already_tracked && (
                <span className="rounded border border-emerald-700 bg-emerald-900/30 px-1 text-[10px] uppercase tracking-wider text-emerald-300">
                  tracked
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
