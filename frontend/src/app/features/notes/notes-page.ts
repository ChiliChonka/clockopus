import { Component, computed, effect, inject, input, signal, untracked } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { formatDuration } from '../../core/format';
import { localDate, startOfDay } from '../../core/local-time';
import { errorMessage, TasksService } from '../../core/tasks.service';
import { NoteView, TaskView } from '../../core/wire';
import { EditTaskDialog } from '../board/edit-task-dialog';

export type Period = '1m' | '3m' | '12m' | 'all' | 'custom';

export const PERIODS: { value: Period; label: string }[] = [
  { value: '1m', label: 'Last month' },
  { value: '3m', label: 'Last 3 months' },
  { value: '12m', label: 'Last 12 months' },
  { value: 'all', label: 'All time' },
  { value: 'custom', label: 'Custom range' },
];

/** The same day `months` earlier, clamped to the month's end (31 Mar − 1 month = 28/29 Feb). */
export function monthsBefore(date: string, months: number): string {
  const d = startOfDay(date);
  const target = new Date(d.getFullYear(), d.getMonth() - months, 1);
  const lastDay = new Date(target.getFullYear(), target.getMonth() + 1, 0).getDate();
  target.setDate(Math.min(d.getDate(), lastDay));
  return localDate(target);
}

/**
 * Local days `[from, to]` of a period ending today; `null` for all time. A custom range without
 * valid dates falls back to the last month.
 */
export function periodRange(
  period: Period,
  today: string,
  from?: string,
  to?: string,
): [string, string] | null {
  const iso = /^\d{4}-\d{2}-\d{2}$/;
  switch (period) {
    case 'all':
      return null;
    case 'custom':
      if (from && to && iso.test(from) && iso.test(to) && from <= to) return [from, to];
      return [monthsBefore(today, 1), today];
    case '3m':
      return [monthsBefore(today, 3), today];
    case '12m':
      return [monthsBefore(today, 12), today];
    default:
      return [monthsBefore(today, 1), today];
  }
}

/** `16 Nov 2026` */
function shortDate(date: string): string {
  return startOfDay(date).toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  });
}

/**
 * Notes view (docs/SPEC.md, 4.5): the notes of all tasks with one tag in a period. The choice
 * lives in the URL (`/notes?tag=team-a&period=3m`), so a reload or a bookmark keeps it.
 */
@Component({
  selector: 'app-notes-page',
  imports: [RouterLink, EditTaskDialog],
  template: `
    <form class="filters" aria-label="Choose notes" (submit)="$event.preventDefault()">
      <label>
        Tag
        <select (change)="choose({ tag: tagSelect.value || null })" #tagSelect>
          <option value="" [selected]="!currentTag()">All tags</option>
          @for (t of tagNames(); track t) {
            <option [value]="t" [selected]="t === currentTag()">#{{ t }}</option>
          }
        </select>
      </label>
      <label>
        Period
        <select (change)="choosePeriod(periodSelect.value)" #periodSelect>
          @for (p of periods; track p.value) {
            <option [value]="p.value" [selected]="p.value === currentPeriod()">
              {{ p.label }}
            </option>
          }
        </select>
      </label>
      @if (currentPeriod() === 'custom') {
        <label>From <input type="date" [value]="range()?.[0]" #fromInput /></label>
        <label>To <input type="date" [value]="range()?.[1]" #toInput /></label>
        <button
          type="submit"
          class="primary"
          (click)="choose({ period: 'custom', from: fromInput.value, to: toInput.value })"
        >
          Show
        </button>
      }
    </form>

    <p class="summary muted" aria-live="polite">
      @if (notes(); as list) {
        {{ list.length }} {{ list.length === 1 ? 'task' : 'tasks' }} with notes
      }
      {{ currentTag() ? 'tagged #' + currentTag() : 'with any tag' }},
      {{ rangeText() }}
    </p>

    @if (error(); as error) {
      <p class="error" role="alert">{{ error }}</p>
    }

    @if (notes(); as list) {
      @for (n of list; track n.id) {
        <article class="note" [class.is-done]="n.status === 'done'" [attr.aria-label]="n.title">
          <header>
            <h2>{{ n.title }}</h2>
            @if (n.link; as link) {
              <a [href]="link" target="_blank" rel="noopener noreferrer" title="Open link">↗</a>
            }
            <button type="button" (click)="editing.set(n)">Edit</button>
          </header>
          <p class="meta">
            @for (t of n.tags; track t) {
              <a class="chip" routerLink="/notes" [queryParams]="params({ tag: t })">#{{ t }}</a>
            }
            <span class="muted">
              @if (n.notes_changed_at) {
                notes changed {{ changed(n.notes_changed_at) }} ·
              }
              {{ hm(n.period_seconds) }} {{ range() ? 'in this period' : 'in total' }}
              @if (n.status === 'done') {
                · done
              }
            </span>
          </p>
          <div class="text">{{ n.notes }}</div>
        </article>
      } @empty {
        <p class="empty muted">
          No notes here. Notes are written in a task's <strong>Edit</strong> dialog on the board;
          tasks show up when they have time, new notes or were created in the period.
        </p>
      }
    }

    <app-edit-task-dialog [task]="editing()" (closed)="editing.set(null)" />
  `,
  styles: `
    :host {
      display: grid;
      gap: 16px;
    }
    .filters {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      align-items: end;
    }
    label {
      display: grid;
      gap: 2px;
      font-size: 0.85rem;
      color: var(--text-muted);
    }
    select,
    input {
      border: 1px solid var(--border);
      border-radius: 8px;
      background: var(--surface);
      padding: 6px 8px;
      color: var(--text);
      font: inherit;
    }
    .summary,
    .empty {
      margin: 0;
    }
    .note {
      display: grid;
      gap: 8px;
      padding: 12px 16px;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: var(--radius);
    }
    header {
      display: flex;
      align-items: start;
      gap: 8px;
    }
    h2 {
      flex: 1;
      margin: 0;
      font-size: 1.05rem;
      overflow-wrap: anywhere;
    }
    .is-done h2 {
      text-decoration: line-through;
      color: var(--text-muted);
    }
    header a {
      color: var(--accent);
      text-decoration: none;
    }
    header button {
      font-size: 0.85rem;
      padding: 2px 10px;
    }
    .meta {
      display: flex;
      flex-wrap: wrap;
      gap: 4px 8px;
      align-items: baseline;
      margin: 0;
      font-size: 0.85rem;
    }
    a.chip {
      color: var(--text);
      text-decoration: none;
    }
    .text {
      white-space: pre-wrap;
      overflow-wrap: anywhere;
      line-height: 1.45;
      border-left: 3px solid var(--border);
      padding-left: 12px;
    }
    .error {
      margin: 0;
      color: var(--danger);
    }
  `,
})
export class NotesPage {
  private readonly tasks = inject(TasksService);
  private readonly router = inject(Router);

  /** Query parameters (withComponentInputBinding). */
  readonly tag = input<string | undefined>();
  readonly period = input<string | undefined>();
  readonly from = input<string | undefined>();
  readonly to = input<string | undefined>();

  readonly periods = PERIODS;
  readonly hm = formatDuration;

  readonly notes = signal<NoteView[] | null>(null);
  readonly error = signal<string | null>(null);
  readonly editing = signal<TaskView | null>(null);

  readonly currentTag = computed(() => this.tag()?.trim().replace(/^#/, '').toLowerCase() || null);
  readonly currentPeriod = computed<Period>(() => {
    const p = this.period();
    return PERIODS.some((x) => x.value === p) ? (p as Period) : '1m';
  });
  /** Today when the page was opened; enough for a view that is reloaded on every visit. */
  private readonly today = localDate(new Date());
  readonly range = computed(() =>
    periodRange(this.currentPeriod(), this.today, this.from(), this.to()),
  );
  readonly rangeText = computed(() => {
    const r = this.range();
    return r ? `${shortDate(r[0])} – ${shortDate(r[1])}` : 'all time';
  });

  /** Known tags by name; the chosen one stays in the list even if it is not known (yet). */
  readonly tagNames = computed(() => {
    const names = this.tasks
      .tags()
      .map((t) => t.name)
      .sort((a, b) => a.localeCompare(b));
    const chosen = this.currentTag();
    return chosen && !names.includes(chosen) ? [chosen, ...names] : names;
  });

  private request = 0;

  constructor() {
    if (!this.tasks.loaded()) void this.tasks.load();
    // Reload when the choice changes or anything was saved (e.g. in the edit dialog).
    effect(() => {
      const tag = this.currentTag();
      const range = this.range();
      this.tasks.revision();
      untracked(() => void this.load(tag, range));
    });
  }

  private async load(tag: string | null, range: [string, string] | null): Promise<void> {
    const mine = ++this.request;
    try {
      const notes = await this.tasks.notes(tag, range);
      if (mine !== this.request) return;
      this.notes.set(notes);
      this.error.set(null);
    } catch (e) {
      if (mine === this.request) this.error.set(errorMessage(e));
    }
  }

  changed(iso: string): string {
    return shortDate(localDate(new Date(iso)));
  }

  /** Query parameters with one change, the rest kept. */
  params(change: { tag?: string | null; period?: Period; from?: string; to?: string }) {
    const p: Record<string, string> = {};
    const tag = 'tag' in change ? change.tag : this.currentTag();
    const period = change.period ?? this.currentPeriod();
    if (tag) p['tag'] = tag;
    if (period !== '1m') p['period'] = period;
    if (period === 'custom') {
      const r = this.range();
      p['from'] = change.from ?? r?.[0] ?? '';
      p['to'] = change.to ?? r?.[1] ?? '';
    }
    return p;
  }

  choose(change: { tag?: string | null; period?: Period; from?: string; to?: string }): void {
    void this.router.navigate(['/notes'], { queryParams: this.params(change) });
  }

  choosePeriod(value: string): void {
    this.choose({ period: (PERIODS.find((p) => p.value === value)?.value ?? '1m') as Period });
  }
}
