/** `h:mm` for cards and timelines (docs/SPEC.md, section 4.3). */
export function formatHm(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds / 60));
  const h = Math.floor(total / 60);
  const m = total % 60;
  return `${h}:${m.toString().padStart(2, '0')}`;
}

/** `3.5 h` for report totals (one decimal). */
export function formatHours(seconds: number): string {
  return `${(Math.max(0, seconds) / 3600).toFixed(1)} h`;
}

/** `23 h 40 min`, `45 min` in sentences. */
export function formatHuman(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds / 60));
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (h === 0) return `${m} min`;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/** `h:mm:ss` for the running timer. */
export function formatHms(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${formatHm(s)}:${(s % 60).toString().padStart(2, '0')}`;
}
