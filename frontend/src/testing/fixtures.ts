import { EntryView, Forgotten, TagView, TaskView } from '../app/core/wire';

export function entryView(over: Partial<EntryView> = {}): EntryView {
  return {
    id: 'e',
    task_id: 'a',
    task_title: 'Fix login bug',
    tags: [],
    start: '2026-11-17T08:00:00Z',
    end: '2026-11-17T09:00:00Z',
    source: 'timer',
    note: null,
    history: [],
    ...over,
  };
}

export function forgotten(over: Partial<Forgotten> = {}): Forgotten {
  return {
    task_id: 'a',
    task_title: 'Fix login bug',
    entry_id: 'e',
    start: '2026-11-16T08:12:00Z',
    running_seconds: 23 * 3600 + 40 * 60,
    suggested_end: '2026-11-16T16:00:00Z',
    ...over,
  };
}

/** Test data in the backend's wire format; only used by specs. */
export function taskView(over: Partial<TaskView> = {}): TaskView {
  return {
    id: 'a',
    title: 'Fix login bug',
    tags: [],
    link: null,
    notes: null,
    notes_changed_at: null,
    status: 'active',
    pinned: false,
    created_at: '2026-11-17T08:00:00Z',
    last_used_at: null,
    running_since: null,
    today_seconds: 0,
    total_seconds: 0,
    ...over,
  };
}

export function tagView(name: string, over: Partial<TagView> = {}): TagView {
  return { name, color: null, task_count: 1, recent_seconds: 0, ...over };
}
