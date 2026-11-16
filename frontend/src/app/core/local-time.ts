// Local time in the browser. The user's time zone setting comes from the browser (first visit),
// so browser-local and user-local agree; the server computes day boundaries on its side.

const pad = (n: number) => n.toString().padStart(2, '0');

/** `YYYY-MM-DD` of a date in local time. */
export function localDate(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `HH:MM` in local time. */
export function localTime(d: Date): string {
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Value for `<input type="datetime-local">`. */
export function toLocalInput(iso: string): string {
  const d = new Date(iso);
  return `${localDate(d)}T${localTime(d)}`;
}

/** Parses `YYYY-MM-DDTHH:MM` (or a date plus `HH:MM`) as local time; `null` if invalid. */
export function fromLocalInput(value: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(value);
  if (!m) return null;
  const d = new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5]);
  return isNaN(d.getTime()) ? null : d;
}

/** Midnight at the start of a local `YYYY-MM-DD`. */
export function startOfDay(date: string): Date {
  const [y, m, d] = date.split('-').map(Number);
  return new Date(y, m - 1, d);
}

export function addDays(date: string, days: number): string {
  const d = startOfDay(date);
  d.setDate(d.getDate() + days);
  return localDate(d);
}

/** `today 09:12`, `yesterday 09:12` or `Mon, 16 Nov 09:12`. */
export function relativeDayTime(iso: string, now: Date): string {
  const d = new Date(iso);
  const day = localDate(d);
  const today = localDate(now);
  const label =
    day === today
      ? 'today'
      : day === addDays(today, -1)
        ? 'yesterday'
        : d.toLocaleDateString('en-GB', { weekday: 'short', day: 'numeric', month: 'short' });
  return `${label} ${localTime(d)}`;
}

/** First and last day of the week containing `date`. */
export function weekRange(date: string, weekStart: 'monday' | 'sunday'): [string, string] {
  const day = startOfDay(date).getDay(); // 0 = Sunday
  const back = weekStart === 'monday' ? (day + 6) % 7 : day;
  const first = addDays(date, -back);
  return [first, addDays(first, 6)];
}

/** First and last day of the month containing `date`. */
export function monthRange(date: string): [string, string] {
  const d = startOfDay(date);
  const first = new Date(d.getFullYear(), d.getMonth(), 1);
  const last = new Date(d.getFullYear(), d.getMonth() + 1, 0);
  return [localDate(first), localDate(last)];
}

/** Whole weeks covering the month of `date`, for a calendar grid. */
export function monthGrid(date: string, weekStart: 'monday' | 'sunday'): string[][] {
  const [first, last] = monthRange(date);
  const [gridStart] = weekRange(first, weekStart);
  const [, gridEnd] = weekRange(last, weekStart);
  const weeks: string[][] = [];
  for (let d = gridStart; d <= gridEnd; d = addDays(d, 7)) {
    weeks.push(Array.from({ length: 7 }, (_, i) => addDays(d, i)));
  }
  return weeks;
}

/** `Mon 16` column headings. */
export function shortDay(date: string): string {
  return startOfDay(date).toLocaleDateString('en-GB', { weekday: 'short', day: 'numeric' });
}

/** `November 2026`. */
export function monthName(date: string): string {
  return startOfDay(date).toLocaleDateString('en-GB', { month: 'long', year: 'numeric' });
}

/** `Monday, 16 November 2026` for headings. */
export function longDate(date: string): string {
  return startOfDay(date).toLocaleDateString('en-GB', {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
    year: 'numeric',
  });
}
