// Wire format of the backend API (backend/src/api.rs). Times are RFC 3339 strings, durations
// whole seconds. Request bodies are built only by the functions below; their tests use the same
// literals as the backend tests, so both sides agree on the format.

export type TaskStatus = 'active' | 'done';

export interface TaskView {
  id: string;
  title: string;
  tags: string[];
  link: string | null;
  status: TaskStatus;
  pinned: boolean;
  created_at: string;
  last_used_at: string | null;
  running_since: string | null;
  today_seconds: number;
  total_seconds: number;
}

export interface TagView {
  name: string;
  color: string | null;
  task_count: number;
  recent_seconds: number;
}

/** R7: the running timer looks forgotten. */
export interface Forgotten {
  task_id: string;
  task_title: string;
  entry_id: string;
  start: string;
  running_seconds: number;
  /** Workday end on the start day, if it lies between start and now. */
  suggested_end: string | null;
}

export interface TimerView {
  running: { task_id: string; entry_id: string; start: string } | null;
  forgotten: Forgotten | null;
}

export type WeekStart = 'monday' | 'sunday';

export interface Settings {
  timezone: string | null;
  /** `HH:MM` */
  workday_end: string;
  forgotten_after_hours: number;
  week_start: WeekStart;
  /** Default CSV rounding in minutes: 0, 5 or 15. */
  csv_rounding: number;
}

/** Body of `PUT /api/v1/me/settings`: only the given fields change. */
export type SettingsPatch = Partial<Omit<Settings, 'timezone'>> & { timezone?: string };

/** `GET /api/v1/reports?from&to` (backend/src/reports.rs). */
export interface Report {
  from: string;
  to: string;
  total_seconds: number;
  days: { date: string; total_seconds: number; by_task: { task_id: string; seconds: number }[] }[];
  by_task: { task_id: string; title: string; tags: string[]; seconds: number }[];
  /** Variant A; `tag: null` is untagged. */
  by_tag: { tag: string | null; seconds: number }[];
}

export type ChangeReason = 'edit' | 'forgotten-timer';

/** Values an entry had before a correction (R6). */
export interface Change {
  changed_at: string;
  reason: ChangeReason;
  start: string;
  end: string | null;
}

export interface EntryView {
  id: string;
  task_id: string;
  task_title: string;
  tags: string[];
  start: string;
  /** `null` while running. */
  end: string | null;
  source: 'timer' | 'manual';
  note: string | null;
  history: Change[];
}

export interface ApiErrorBody {
  error: string;
  message?: string;
  /** The other entry of an overlap (R5). */
  entry_id?: string;
}

export interface AddEntryWire {
  start: string;
  end: string;
  note?: string;
}

/** Body of `POST /api/v1/tasks/{id}/entries`. */
export function toAddEntryWire(start: Date, end: Date, note: string): AddEntryWire {
  const out: AddEntryWire = { start: start.toISOString(), end: end.toISOString() };
  if (note.trim()) out.note = note.trim();
  return out;
}

export interface UpdateEntryWire {
  start?: string;
  end?: string;
  note?: string;
}

/** Body of `PATCH /api/v1/entries/{id}`; `note: ''` removes the note. */
export function toUpdateEntryWire(patch: {
  start?: Date;
  end?: Date;
  note?: string;
}): UpdateEntryWire {
  const out: UpdateEntryWire = {};
  if (patch.start) out.start = patch.start.toISOString();
  if (patch.end) out.end = patch.end.toISOString();
  if (patch.note !== undefined) out.note = patch.note.trim();
  return out;
}

/** Body of `POST /api/v1/timer/stop` for the R8 correction of a forgotten timer. */
export function toStopForgottenWire(end: Date): { end: string; reason: ChangeReason } {
  return { end: end.toISOString(), reason: 'forgotten-timer' };
}

export interface CreateTaskWire {
  title: string;
  tags: string[];
  start: boolean;
}

/** Body of `POST /api/v1/tasks`. */
export function toCreateTaskWire(title: string, tags: string[], start: boolean): CreateTaskWire {
  return { title, tags, start };
}

export interface UpdateTaskWire {
  title?: string;
  tags?: string[];
  link?: string;
  pinned?: boolean;
  status?: TaskStatus;
}

/** Body of `PATCH /api/v1/tasks/{id}`; only the given fields change. */
export function toUpdateTaskWire(patch: UpdateTaskWire): UpdateTaskWire {
  const out: UpdateTaskWire = {};
  if (patch.title !== undefined) out.title = patch.title;
  if (patch.tags !== undefined) out.tags = patch.tags;
  if (patch.link !== undefined) out.link = patch.link;
  if (patch.pinned !== undefined) out.pinned = patch.pinned;
  if (patch.status !== undefined) out.status = patch.status;
  return out;
}
