import { useEffect, useState } from "react";
import { api, Project, UpdateFields } from "../api";
import { StatusBadge } from "./StatusBadge";

const MANUAL_STATUS_OPTIONS = [
  "",
  "planning",
  "developing",
  "deployed",
  "archived",
];

export function ProjectDetail({
  project,
  onChange,
  onResync,
}: {
  project: Project;
  onChange: (fields: Partial<UpdateFields>) => Promise<void>;
  onResync: () => Promise<void>;
}) {
  const [form, setForm] = useState<{
    deploy_url: string;
    deploy_instructions: string;
    launch_instructions: string;
    notes: string;
    status: string;
  }>({
    deploy_url: project.deploy_url ?? "",
    deploy_instructions: project.deploy_instructions ?? "",
    launch_instructions: project.launch_instructions ?? "",
    notes: project.notes ?? "",
    status: project.status_manual && project.status ? project.status : "",
  });
  const [saving, setSaving] = useState(false);
  const [launchErr, setLaunchErr] = useState<string | null>(null);

  useEffect(() => {
    setForm({
      deploy_url: project.deploy_url ?? "",
      deploy_instructions: project.deploy_instructions ?? "",
      launch_instructions: project.launch_instructions ?? "",
      notes: project.notes ?? "",
      status: project.status_manual && project.status ? project.status : "",
    });
    // Only resetting when the user switches to a different project — otherwise
    // an in-flight refresh after `onChange` would clobber unsaved edits.
  }, [project.id]);

  const save = async () => {
    setSaving(true);
    try {
      await onChange({
        deploy_url: form.deploy_url || null,
        deploy_instructions: form.deploy_instructions || null,
        launch_instructions: form.launch_instructions || null,
        notes: form.notes || null,
        status: form.status || null,
        status_manual: form.status ? true : false,
      });
    } finally {
      setSaving(false);
    }
  };

  const handleStart = async () => {
    setLaunchErr(null);
    try {
      await api.startClaude(project.id);
    } catch (e) {
      setLaunchErr(String(e));
    }
  };

  return (
    <div className="flex flex-col gap-4 p-6 font-mono text-sm">
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <h2 className="text-xl font-semibold text-zinc-100">{project.name}</h2>
          <StatusBadge project={project} />
        </div>
        <div className="flex gap-2 text-xs">
          <button
            onClick={handleStart}
            title={`Open a terminal at ${project.path} and run claude`}
            className="rounded bg-emerald-600/80 px-2 py-1 font-semibold text-white hover:bg-emerald-600"
          >
            Start
          </button>
          <button
            onClick={() => api.openInFinder(project.path)}
            className="rounded border border-zinc-700 px-2 py-1 text-zinc-300 hover:bg-zinc-800"
          >
            Open folder
          </button>
          {project.github_url && (
            <button
              onClick={() => api.openUrl(project.github_url!)}
              className="rounded border border-zinc-700 px-2 py-1 text-zinc-300 hover:bg-zinc-800"
            >
              GitHub
            </button>
          )}
          {project.deploy_url && (
            <button
              onClick={() => api.openUrl(project.deploy_url!)}
              className="rounded border border-zinc-700 px-2 py-1 text-zinc-300 hover:bg-zinc-800"
            >
              Deploy
            </button>
          )}
          <button
            onClick={onResync}
            className="rounded border border-zinc-700 px-2 py-1 text-zinc-300 hover:bg-zinc-800"
          >
            Resync
          </button>
        </div>
      </div>

      {launchErr && (
        <div className="rounded border border-red-700 bg-red-900/30 p-2 text-xs text-red-300">
          {launchErr}
        </div>
      )}

      <section className="grid grid-cols-2 gap-x-6 gap-y-2 rounded border border-zinc-800 bg-zinc-900 p-4 text-xs">
        <KV label="Path" value={project.path} />
        <KV label="GitHub" value={project.github_url ?? "—"} />
        <KV label="Last active" value={fmt(project.last_active_at)} />
        <KV label="First seen" value={fmt(project.first_seen_at)} />
        <KV label="Sessions" value={String(project.sessions_started)} />
        <KV label="Prompts" value={String(project.prompts_count)} />
        <KV label="Deploy platform" value={project.deploy_platform ?? "—"} />
        <KV
          label="Last enriched"
          value={fmt(project.enrichment_synced_at)}
        />
      </section>

      <section className="flex flex-col gap-3 rounded border border-zinc-800 bg-zinc-900 p-4 text-xs">
        <Field
          label="Status (manual override)"
          help="Leave blank to auto-infer from activity."
        >
          <select
            value={form.status}
            onChange={(e) => setForm({ ...form, status: e.target.value })}
            className="w-48 rounded border border-zinc-700 bg-zinc-950 px-2 py-1"
          >
            {MANUAL_STATUS_OPTIONS.map((s) => (
              <option key={s} value={s}>
                {s || "(auto)"}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Deploy URL">
          <input
            value={form.deploy_url}
            onChange={(e) => setForm({ ...form, deploy_url: e.target.value })}
            placeholder="https://example.com"
            className="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1"
          />
        </Field>
        <Field label="Deploy instructions">
          <textarea
            value={form.deploy_instructions}
            onChange={(e) =>
              setForm({ ...form, deploy_instructions: e.target.value })
            }
            placeholder="vercel --prod"
            rows={3}
            className="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1"
          />
        </Field>
        <Field
          label="Launch instructions"
          help="Auto-inferred on sync. You can override here."
        >
          <textarea
            value={form.launch_instructions}
            onChange={(e) =>
              setForm({ ...form, launch_instructions: e.target.value })
            }
            rows={3}
            className="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1"
          />
        </Field>
        <Field label="Notes">
          <textarea
            value={form.notes}
            onChange={(e) => setForm({ ...form, notes: e.target.value })}
            rows={3}
            className="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1"
          />
        </Field>

        <div className="flex items-center justify-between pt-2">
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={project.deploy_live_lookup}
              onChange={(e) =>
                onChange({ deploy_live_lookup: e.target.checked })
              }
            />
            <span className="text-zinc-300">
              Live deploy lookup (shell out to vercel/netlify/fly on sync)
            </span>
          </label>
          <button
            onClick={save}
            disabled={saving}
            className="rounded bg-emerald-600/80 px-3 py-1 text-xs font-semibold text-white hover:bg-emerald-600 disabled:opacity-50"
          >
            {saving ? "saving…" : "Save"}
          </button>
        </div>
      </section>
    </div>
  );
}

function KV({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] uppercase tracking-wider text-zinc-500">
        {label}
      </span>
      <span className="truncate text-zinc-200">{value}</span>
    </div>
  );
}

function Field({
  label,
  help,
  children,
}: {
  label: string;
  help?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[10px] uppercase tracking-wider text-zinc-500">
        {label}
      </span>
      {children}
      {help && <span className="text-[10px] text-zinc-600">{help}</span>}
    </div>
  );
}

function fmt(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString();
}
