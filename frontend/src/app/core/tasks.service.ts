import { HttpClient, HttpErrorResponse } from '@angular/common/http';
import { Injectable, computed, inject, signal } from '@angular/core';
import { firstValueFrom } from 'rxjs';
import { parseQuickAdd } from './quick-add';
import {
  ApiErrorBody,
  EntryView,
  TagView,
  TaskView,
  TimerView,
  UpdateTaskWire,
  toAddEntryWire,
  toCreateTaskWire,
  toStopForgottenWire,
  toUpdateEntryWire,
  toUpdateTaskWire,
} from './wire';

/** Tasks, tags and the timer of the signed-in user. Every change reloads the board. */
@Injectable({ providedIn: 'root' })
export class TasksService {
  private readonly http = inject(HttpClient);

  readonly tasks = signal<TaskView[]>([]);
  readonly tags = signal<TagView[]>([]);
  readonly showDone = signal(false);
  readonly error = signal<string | null>(null);
  readonly loaded = signal(false);
  /** When `tasks` was loaded (ms); live times add the time since then. */
  readonly loadedAt = signal(0);

  readonly timer = signal<TimerView>({ running: null, forgotten: null });
  /** Bumped after every change, so other views (day timeline) know to reload. */
  readonly revision = signal(0);

  readonly running = computed(() => this.tasks().find((t) => t.running_since !== null) ?? null);
  /** R7: set when the running timer looks forgotten. */
  readonly forgotten = computed(() => this.timer().forgotten);
  /** The 8 tags with the most time in the last 30 days (server order). */
  readonly quickTags = computed(() => this.tags().slice(0, 8));

  /** Reloads the board. Leaves an error from a previous action visible on success. */
  async load(): Promise<void> {
    try {
      const status = this.showDone() ? 'all' : 'active';
      const [tasks, tags, timer] = await Promise.all([
        firstValueFrom(this.http.get<TaskView[]>('/api/v1/tasks', { params: { status } })),
        firstValueFrom(this.http.get<TagView[]>('/api/v1/tags')),
        firstValueFrom(this.http.get<TimerView>('/api/v1/timer')),
      ]);
      this.tasks.set(tasks);
      this.tags.set(tags);
      this.timer.set(timer);
      this.loadedAt.set(Date.now());
      this.loaded.set(true);
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  /** Creates a task from quick-add text. Returns false if nothing was created. */
  async create(text: string, start: boolean): Promise<boolean> {
    const { title, tags } = parseQuickAdd(text);
    if (!title) {
      this.error.set('Give the task a title; words starting with # become tags.');
      return false;
    }
    return this.run(() =>
      firstValueFrom(
        this.http.post<TaskView>('/api/v1/tasks', toCreateTaskWire(title, tags, start)),
      ),
    );
  }

  start(id: string): Promise<boolean> {
    return this.run(() =>
      firstValueFrom(this.http.post<TimerView>(`/api/v1/tasks/${id}/start`, {})),
    );
  }

  stop(): Promise<boolean> {
    return this.run(() => firstValueFrom(this.http.post<TimerView>('/api/v1/timer/stop', {})));
  }

  update(id: string, patch: UpdateTaskWire): Promise<boolean> {
    return this.run(() =>
      firstValueFrom(this.http.patch<TaskView>(`/api/v1/tasks/${id}`, toUpdateTaskWire(patch))),
    );
  }

  remove(id: string): Promise<boolean> {
    return this.run(() => firstValueFrom(this.http.delete<void>(`/api/v1/tasks/${id}`)));
  }

  async setShowDone(show: boolean): Promise<void> {
    this.showDone.set(show);
    await this.load();
  }

  /** R8: stop the forgotten timer at `end`, recorded as a forgotten-timer correction. */
  stopForgotten(end: Date): Promise<boolean> {
    return this.run(() =>
      firstValueFrom(this.http.post<TimerView>('/api/v1/timer/stop', toStopForgottenWire(end))),
    );
  }

  /** R8 “Keep running”. */
  keepRunning(): Promise<boolean> {
    return this.run(() =>
      firstValueFrom(this.http.post<TimerView>('/api/v1/timer/keep-running', {})),
    );
  }

  /** Entries of one task, newest first. */
  entries(taskId: string): Promise<EntryView[]> {
    return firstValueFrom(this.http.get<EntryView[]>(`/api/v1/tasks/${taskId}/entries`));
  }

  /** Entries touching the local days `from..=to` (`YYYY-MM-DD`). */
  entriesBetween(from: string, to: string): Promise<EntryView[]> {
    return firstValueFrom(this.http.get<EntryView[]>('/api/v1/entries', { params: { from, to } }));
  }

  addEntry(taskId: string, start: Date, end: Date, note: string): Promise<boolean> {
    return this.run(() =>
      firstValueFrom(
        this.http.post<EntryView>(
          `/api/v1/tasks/${taskId}/entries`,
          toAddEntryWire(start, end, note),
        ),
      ),
    );
  }

  updateEntry(id: string, patch: { start?: Date; end?: Date; note?: string }): Promise<boolean> {
    return this.run(() =>
      firstValueFrom(this.http.patch<EntryView>(`/api/v1/entries/${id}`, toUpdateEntryWire(patch))),
    );
  }

  deleteEntry(id: string): Promise<boolean> {
    return this.run(() => firstValueFrom(this.http.delete<void>(`/api/v1/entries/${id}`)));
  }

  clearError(): void {
    this.error.set(null);
  }

  /** Runs an action, then reloads the board (also after a failure, the data may have changed). */
  private async run(op: () => Promise<unknown>): Promise<boolean> {
    let ok = true;
    try {
      await op();
      this.error.set(null);
    } catch (e) {
      ok = false;
      this.error.set(errorMessage(e));
    }
    await this.load();
    this.revision.update((r) => r + 1);
    return ok;
  }
}

export function errorMessage(e: unknown): string {
  if (e instanceof HttpErrorResponse) {
    const body = e.error as ApiErrorBody | null;
    if (body && typeof body === 'object' && typeof body.message === 'string') return body.message;
    if (e.status === 0) return 'The clockopus server is not reachable.';
  }
  return 'Something went wrong. Please try again.';
}
