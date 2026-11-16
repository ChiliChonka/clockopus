import {
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { Title } from '@angular/platform-browser';
import { formatHm } from '../../core/format';
import { SessionService } from '../../core/session.service';
import { TasksService } from '../../core/tasks.service';
import { TaskView } from '../../core/wire';
import { QuickAddField, QuickAddSubmit } from './quick-add-field';
import { TaskCard } from './task-card';

const RELOAD_EVERY_MS = 5 * 60 * 1000;

/** The board (docs/SPEC.md, section 4.1): quick add, frequent tags, filter and task cards. */
@Component({
  selector: 'app-board',
  imports: [QuickAddField, TaskCard],
  host: {
    '(document:keydown)': 'onShortcut($event)',
    '(document:visibilitychange)': 'onVisible()',
  },
  template: `
    <app-quick-add-field
      [(text)]="text"
      [knownTags]="tasks.tags()"
      [quickTags]="tasks.quickTags()"
      (submitted)="create($event)"
    />

    @if (tasks.error(); as error) {
      <p class="error" role="alert">{{ error }}</p>
    }

    @if (tasks.loaded() && (tasks.tasks().length || tasks.showDone())) {
      <div class="filters">
        <label class="sr-only" for="filter">Filter tasks</label>
        <input
          id="filter"
          type="search"
          placeholder="Filter by title or #tag"
          [value]="filter()"
          (input)="filter.set(filterInput.value)"
          #filterInput
        />
        <label class="toggle">
          <input
            type="checkbox"
            [checked]="tasks.showDone()"
            (change)="tasks.setShowDone(doneInput.checked)"
            #doneInput
          />
          Show done
        </label>
      </div>
    }

    @if (tasks.loaded() && !tasks.tasks().length && !tasks.showDone()) {
      <section class="empty">
        <img src="logo.svg" alt="" width="120" height="120" />
        <h2>No tasks yet</h2>
        <p>Type what you are working on above and press Enter. The timer starts right away.</p>
      </section>
    } @else {
      <div class="cards">
        @for (task of visible(); track task.id) {
          <app-task-card
            [task]="task"
            [now]="now()"
            [loadedAt]="tasks.loadedAt()"
            (start)="tasks.start(task.id)"
            (stop)="tasks.stop()"
            (pin)="tasks.update(task.id, { pinned: $event })"
            (done)="tasks.update(task.id, { status: 'done' })"
            (remove)="remove(task)"
          />
        } @empty {
          @if (tasks.loaded()) {
            <p class="muted">No task matches “{{ filter() }}”.</p>
          }
        }
      </div>
    }
  `,
  styles: `
    :host {
      display: grid;
      gap: 16px;
    }
    .error {
      margin: 0;
      padding: 10px 14px;
      border: 1px solid var(--danger);
      border-radius: var(--radius);
      color: var(--danger);
    }
    .filters {
      display: flex;
      gap: 12px;
      align-items: center;
    }
    .toggle {
      display: flex;
      gap: 6px;
      align-items: center;
      white-space: nowrap;
    }
    .cards {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
      gap: 12px;
    }
    .empty {
      display: grid;
      justify-items: center;
      gap: 8px;
      padding: 48px 16px;
      text-align: center;
      color: var(--text-muted);
    }
    .empty h2 {
      margin: 8px 0 0;
      color: var(--text);
      font-size: 1.25rem;
    }
    .empty p {
      margin: 0;
      max-width: 32rem;
    }
  `,
})
export class Board {
  protected readonly tasks = inject(TasksService);
  private readonly session = inject(SessionService);
  private readonly title = inject(Title);
  private readonly field = viewChild.required(QuickAddField);

  readonly text = signal('');
  readonly filter = signal('');
  readonly now = signal(Date.now());

  readonly visible = computed(() => {
    const words = this.filter().toLowerCase().split(/\s+/).filter(Boolean);
    return this.tasks
      .tasks()
      .filter((t) =>
        words.every((w) =>
          w.startsWith('#')
            ? t.tags.some((tag) => tag.toLowerCase().startsWith(w.slice(1)))
            : t.title.toLowerCase().includes(w) ||
              t.tags.some((tag) => tag.toLowerCase().includes(w)),
        ),
      );
  });

  constructor() {
    // Load once the session (and with it the time zone) is ready.
    effect(() => {
      if (this.session.me()) untracked(() => void this.tasks.load());
    });

    // Running timer in the tab title: `▶ 0:42 Fix login bug`.
    effect(() => {
      const running = this.tasks.running();
      const now = this.now();
      this.title.setTitle(
        running?.running_since
          ? `▶ ${formatHm((now - Date.parse(running.running_since)) / 1000)} ${running.title}`
          : 'clockopus',
      );
    });

    const tick = setInterval(() => this.now.set(Date.now()), 1000);
    // Keeps "today" right across midnight and picks up changes from other tabs.
    const reload = setInterval(() => void this.tasks.load(), RELOAD_EVERY_MS);
    inject(DestroyRef).onDestroy(() => {
      clearInterval(tick);
      clearInterval(reload);
      this.title.setTitle('clockopus');
    });
  }

  async create(submit: QuickAddSubmit): Promise<void> {
    if (await this.tasks.create(submit.text, submit.start)) this.text.set('');
  }

  remove(task: TaskView): void {
    const tracked = task.total_seconds > 0 ? ` and its ${formatHm(task.total_seconds)} h` : '';
    if (confirm(`Delete “${task.title}”${tracked}? This cannot be undone.`)) {
      void this.tasks.remove(task.id);
    }
  }

  /** `n` or Ctrl/⌘+K focuses the quick-add field. */
  onShortcut(event: KeyboardEvent): void {
    const target = event.target as HTMLElement | null;
    const typing =
      target?.closest('input, textarea, select, [contenteditable="true"]') !== null &&
      target !== null;
    const ctrlK = (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k';
    const plainN = event.key === 'n' && !event.ctrlKey && !event.metaKey && !event.altKey;
    if (ctrlK || (plainN && !typing)) {
      event.preventDefault();
      this.field().focus();
    }
  }

  onVisible(): void {
    if (document.visibilityState === 'visible' && this.session.me()) void this.tasks.load();
  }
}
