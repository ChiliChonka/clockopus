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

export interface TimerView {
  running: { task_id: string; entry_id: string; start: string } | null;
}

export interface Settings {
  timezone: string | null;
}

export interface ApiErrorBody {
  error: string;
  message?: string;
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
