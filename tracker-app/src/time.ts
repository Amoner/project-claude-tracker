export function fmtAbsolute(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString();
}

export function fmtRelative(iso: string | null): string {
  if (!iso) return "never";
  const delta = Date.now() - new Date(iso).getTime();
  const h = delta / 3_600_000;
  if (h < 1) return `${Math.max(1, Math.round(delta / 60_000))}m ago`;
  if (h < 24) return `${Math.round(h)}h ago`;
  const days = h / 24;
  if (days < 30) return `${Math.round(days)}d ago`;
  return new Date(iso).toLocaleDateString();
}
