import { TagView, TaskView } from '../app/core/wire';

/** Test data in the backend's wire format; only used by specs. */
export function taskView(over: Partial<TaskView> = {}): TaskView {
  return {
    id: 'a',
    title: 'Fix login bug',
    tags: [],
    link: null,
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
