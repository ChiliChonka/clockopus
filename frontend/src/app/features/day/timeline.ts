// Geometry and totals of the day timeline (docs/SPEC.md, section 4.3), as pure functions.
// Minutes count from local midnight of the day; a day with a DST switch has 1380 or 1500.

import { addDays, startOfDay } from '../../core/local-time';
import { EntryView } from '../../core/wire';

export interface Block {
  entry: EntryView;
  /** Minutes from midnight, clipped to the day. */
  startMin: number;
  endMin: number;
  running: boolean;
  /** The entry began the day before / goes on into the next day. */
  clippedStart: boolean;
  clippedEnd: boolean;
  /** Colour slot 1..8, by first appearance of the task on this day. */
  color: number;
}

export const SNAP_MINUTES = 5;

export function dayMinutes(day: string): number {
  return (startOfDay(addDays(day, 1)).getTime() - startOfDay(day).getTime()) / 60000;
}

export function blocksForDay(entries: EntryView[], day: string, now: Date): Block[] {
  const midnight = startOfDay(day).getTime();
  const length = dayMinutes(day);
  const colors = new Map<string, number>();
  return entries
    .map((entry) => {
      const start = (new Date(entry.start).getTime() - midnight) / 60000;
      const end = ((entry.end ? new Date(entry.end) : now).getTime() - midnight) / 60000;
      if (!colors.has(entry.task_id)) colors.set(entry.task_id, (colors.size % 8) + 1);
      return {
        entry,
        startMin: Math.max(0, start),
        endMin: Math.min(length, end),
        running: entry.end === null,
        clippedStart: start < 0,
        clippedEnd: end > length,
        color: colors.get(entry.task_id)!,
      };
    })
    .filter((b) => b.endMin > b.startMin);
}

/** Visible hours: at least 08–18, widened to whole hours around all blocks. */
export function hourRange(blocks: Block[], day: string): [number, number] {
  let from = 8;
  let to = 18;
  for (const b of blocks) {
    from = Math.min(from, Math.floor(b.startMin / 60));
    to = Math.max(to, Math.ceil(b.endMin / 60));
  }
  return [from, Math.min(to, Math.ceil(dayMinutes(day) / 60))];
}

export function snap(minutes: number, step = SNAP_MINUTES): number {
  return Math.round(minutes / step) * step;
}

/** Instant of a minute offset on the day. */
export function minuteToDate(day: string, minutes: number): Date {
  return new Date(startOfDay(day).getTime() + minutes * 60000);
}

export interface Totals {
  totalSeconds: number;
  byTask: { taskId: string; title: string; seconds: number; color: number }[];
  /** Variant A: every tag of a task gets the task's full time; `null` = untagged. */
  byTag: { tag: string | null; seconds: number }[];
}

export function totals(blocks: Block[]): Totals {
  const byTask = new Map<string, Totals['byTask'][number]>();
  const byTag = new Map<string | null, number>();
  let totalSeconds = 0;
  for (const b of blocks) {
    const seconds = Math.round((b.endMin - b.startMin) * 60);
    totalSeconds += seconds;
    const task = byTask.get(b.entry.task_id) ?? {
      taskId: b.entry.task_id,
      title: b.entry.task_title,
      seconds: 0,
      color: b.color,
    };
    task.seconds += seconds;
    byTask.set(b.entry.task_id, task);
    const tags = b.entry.tags.length ? b.entry.tags : [null];
    for (const tag of tags) byTag.set(tag, (byTag.get(tag) ?? 0) + seconds);
  }
  return {
    totalSeconds,
    byTask: [...byTask.values()].sort((a, b) => b.seconds - a.seconds),
    byTag: [...byTag.entries()]
      .map(([tag, seconds]) => ({ tag, seconds }))
      .sort((a, b) => b.seconds - a.seconds || (a.tag ?? '~').localeCompare(b.tag ?? '~')),
  };
}
