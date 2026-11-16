import { HttpClient, HttpErrorResponse } from '@angular/common/http';
import { Injectable, computed, inject, signal } from '@angular/core';
import { firstValueFrom } from 'rxjs';
import { parseQuickAdd } from './quick-add';
import {
  ApiErrorBody,
  TagView,
  TaskView,
  TimerView,
  UpdateTaskWire,
  toCreateTaskWire,
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

  readonly running = computed(() => this.tasks().find((t) => t.running_since !== null) ?? null);
  /** The 8 tags with the most time in the last 30 days (server order). */
  readonly quickTags = computed(() => this.tags().slice(0, 8));

  /** Reloads the board. Leaves an error from a previous action visible on success. */
  async load(): Promise<void> {
    try {
      const status = this.showDone() ? 'all' : 'active';
      const [tasks, tags] = await Promise.all([
        firstValueFrom(this.http.get<TaskView[]>('/api/v1/tasks', { params: { status } })),
        firstValueFrom(this.http.get<TagView[]>('/api/v1/tags')),
      ]);
      this.tasks.set(tasks);
      this.tags.set(tags);
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
