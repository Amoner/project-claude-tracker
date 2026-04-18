import { invoke } from "@tauri-apps/api/core";

export type Project = {
  id: number;
  path: string;
  name: string;
  status: string | null;
  status_manual: boolean;
  github_url: string | null;
  deploy_url: string | null;
  deploy_platform: string | null;
  deploy_instructions: string | null;
  launch_instructions: string | null;
  deploy_live_lookup: boolean;
  first_seen_at: string;
  last_active_at: string | null;
  sessions_started: number;
  prompts_count: number;
  notes: string | null;
  enrichment_synced_at: string | null;
  archived_at: string | null;
};

export type HookStatus = {
  settings_path: string;
  installed_events: string[];
  cli_path: string | null;
  fully_installed: boolean;
};

export type TerminalInfo = {
  slug: string;
  display_name: string;
  installed: boolean;
};

export const api = {
  listProjects: (includeArchived = false) =>
    invoke<Project[]>("list_projects", { includeArchived }),
  getProject: (id: number) => invoke<Project | null>("get_project", { id }),
  updateProject: (id: number, fields: Partial<UpdateFields>) =>
    invoke<Project>("update_project", { id, fields }),
  runSync: (force = false, liveLookup = false) =>
    invoke<number>("run_sync", { force, liveLookup }),
  runDiscover: () => invoke<number>("run_discover"),
  getHookStatus: () => invoke<HookStatus>("get_hook_status"),
  installHooks: () => invoke<HookStatus>("install_hooks"),
  uninstallHooks: () => invoke<HookStatus>("uninstall_hooks"),
  openInFinder: (path: string) => invoke<null>("open_in_finder", { path }),
  openUrl: (url: string) => invoke<null>("open_url", { url }),
  recentActive: (limit: number) =>
    invoke<Project[]>("recent_active", { limit }),
  listTerminals: () => invoke<TerminalInfo[]>("list_terminals"),
  getPreferredTerminal: () =>
    invoke<string | null>("get_preferred_terminal"),
  setPreferredTerminal: (terminal: string) =>
    invoke<null>("set_preferred_terminal", { terminal }),
  startClaude: (id: number) => invoke<null>("start_claude", { id }),
};

export type UpdateFields = {
  name: string | null;
  status: string | null;
  status_manual: boolean | null;
  deploy_url: string | null;
  deploy_instructions: string | null;
  launch_instructions: string | null;
  notes: string | null;
  deploy_live_lookup: boolean | null;
  archived: boolean | null;
};
