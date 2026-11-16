/** `h:mm` for cards and timelines (docs/SPEC.md, section 4.3). */
export function formatHm(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds / 60));
  const h = Math.floor(total / 60);
  const m = total % 60;
  return `${h}:${m.toString().padStart(2, '0')}`;
}

/** `h:mm:ss` for the running timer. */
export function formatHms(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${formatHm(s)}:${(s % 60).toString().padStart(2, '0')}`;
}
