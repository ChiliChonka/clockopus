import {
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
} from '@angular/core';
import { RouterLink } from '@angular/router';
import { formatHm, formatHours } from '../../core/format';
import { addDays, localDate, localTime, longDate } from '../../core/local-time';
import { SessionService } from '../../core/session.service';
import { TasksService } from '../../core/tasks.service';
import { EntryView } from '../../core/wire';
import { EntryChange, EntryRow } from '../entries/entry-row';
import {
  Block,
  SNAP_MINUTES,
  blocksForDay,
  dayMinutes,
  hourRange,
  minuteToDate,
  snap,
  totals,
} from './timeline';

/** Pixels per minute on the timeline. */
const PX = 1;

interface Drag {
  block: Block;
  edge: 'start' | 'end';
  startY: number;
  origin: number;
}

/** Day view (docs/SPEC.md, 4.3): timeline with draggable edges, totals per task and tag. */
@Component({
  selector: 'app-day-view',
  imports: [EntryRow, RouterLink],
  template: `
    <nav class="days" aria-label="Choose day">
      <a [routerLink]="['/day', previous()]" aria-label="Previous day">‹</a>
      <h2>{{ heading() }}</h2>
      <a [routerLink]="['/day', next()]" aria-label="Next day">›</a>
      @if (day() !== today()) {
        <a [routerLink]="['/day', today()]" class="today">Today</a>
      }
    </nav>

    @if (tasks.error(); as error) {
      <p class="error" role="alert">{{ error }}</p>
    }

    <div class="layout">
      <section aria-label="Timeline">
        @if (blocks().length) {
          <div
            class="timeline"
            [style.height.px]="(range()[1] - range()[0]) * 60 * px"
            (pointermove)="onPointerMove($event)"
            (pointerup)="onPointerUp()"
            (pointercancel)="cancelDrag()"
          >
            @for (h of hours(); track h) {
              <div class="hour" [style.top.px]="(h - range()[0]) * 60 * px">
                <span>{{ h.toString().padStart(2, '0') }}:00</span>
              </div>
            }
            @for (b of blocks(); track b.entry.id) {
              <div
                class="block c{{ b.color }}"
                [class.selected]="selectedId() === b.entry.id"
                [class.running]="b.running"
                [style.top.px]="top(b)"
                [style.height.px]="height(b)"
              >
                @if (!b.clippedStart) {
                  <span
                    class="handle top"
                    (pointerdown)="onPointerDown($event, b, 'start')"
                    title="Drag to change the start"
                  ></span>
                }
                <button type="button" (click)="selectedId.set(b.entry.id)">
                  <strong>{{ b.entry.task_title }}</strong>
                  <span>{{ label(b) }}</span>
                </button>
                @if (!b.clippedEnd && !b.running) {
                  <span
                    class="handle bottom"
                    (pointerdown)="onPointerDown($event, b, 'end')"
                    title="Drag to change the end"
                  ></span>
                }
              </div>
            }
          </div>
        } @else if (loaded()) {
          <p class="muted empty">Nothing tracked on this day.</p>
        }

        @if (selected(); as entry) {
          <div class="editor">
            <h3>{{ entry.task_title }}</h3>
            <app-entry-row [entry]="entry" (save)="save(entry, $event)" (remove)="remove(entry)" />
          </div>
        }
      </section>

      <aside aria-label="Totals">
        <p class="total">
          Total <strong>{{ hm(sums().totalSeconds) }}</strong>
        </p>
        <ul class="list">
          @for (t of sums().byTask; track t.taskId) {
            <li>
              <span class="swatch c{{ t.color }}"></span>
              <span class="name">{{ t.title }}</span>
              <span class="num">{{ hm(t.seconds) }}</span>
            </li>
          }
        </ul>
        @if (sums().byTag.length) {
          <h3>Hours per tag</h3>
          <ul class="list">
            @for (t of sums().byTag; track t.tag) {
              <li>
                <span class="name">{{ t.tag ? '#' + t.tag : 'untagged' }}</span>
                <span class="num">{{ tagHours(t.seconds) }}</span>
              </li>
            }
          </ul>
          <p class="muted hint">A task with several tags counts fully for each of them.</p>
        }
      </aside>
    </div>
  `,
  styleUrl: './day-view.css',
})
export class DayView {
  protected readonly tasks = inject(TasksService);
  private readonly session = inject(SessionService);
  protected readonly px = PX;

  /** Route parameter `YYYY-MM-DD`; today when missing or invalid. */
  readonly date = input<string | undefined>(undefined);

  readonly now = signal(Date.now());
  readonly entries = signal<EntryView[]>([]);
  readonly loaded = signal(false);
  readonly selectedId = signal<string | null>(null);
  /** Edge position while dragging, in minutes. */
  readonly preview = signal<{ id: string; edge: 'start' | 'end'; minutes: number } | null>(null);
  private drag: Drag | null = null;

  readonly today = computed(() => localDate(new Date(this.now())));
  readonly day = computed(() => {
    const d = this.date();
    return d && /^\d{4}-\d{2}-\d{2}$/.test(d) ? d : this.today();
  });
  readonly heading = computed(() => longDate(this.day()));
  readonly previous = computed(() => addDays(this.day(), -1));
  readonly next = computed(() => addDays(this.day(), 1));

  readonly blocks = computed(() => {
    const p = this.preview();
    return blocksForDay(this.entries(), this.day(), new Date(this.now())).map((b) =>
      p && p.id === b.entry.id
        ? { ...b, [p.edge === 'start' ? 'startMin' : 'endMin']: p.minutes }
        : b,
    );
  });
  readonly range = computed(() => hourRange(this.blocks(), this.day()));
  readonly hours = computed(() => {
    const [from, to] = this.range();
    return Array.from({ length: to - from + 1 }, (_, i) => from + i);
  });
  readonly sums = computed(() => totals(this.blocks()));
  readonly selected = computed(
    () => this.entries().find((e) => e.id === this.selectedId()) ?? null,
  );

  constructor() {
    effect(() => {
      const day = this.day();
      this.tasks.revision();
      if (this.session.me()) untracked(() => void this.load(day));
    });
    const tick = setInterval(() => this.now.set(Date.now()), 30_000);
    inject(DestroyRef).onDestroy(() => clearInterval(tick));
  }

  private async load(day: string): Promise<void> {
    try {
      this.entries.set(await this.tasks.entriesBetween(day, day));
      this.loaded.set(true);
    } catch {
      this.tasks.error.set('The entries of this day could not be loaded.');
    }
  }

  protected readonly hm = formatHm;
  protected readonly tagHours = formatHours;

  top(b: Block): number {
    return (b.startMin - this.range()[0] * 60) * PX;
  }

  height(b: Block): number {
    return Math.max(6, (b.endMin - b.startMin) * PX);
  }

  label(b: Block): string {
    const start = localTime(minuteToDate(this.day(), b.startMin));
    const end = b.running ? 'now' : localTime(minuteToDate(this.day(), b.endMin));
    return `${start}–${end} · ${formatHm((b.endMin - b.startMin) * 60)}`;
  }

  onPointerDown(event: PointerEvent, block: Block, edge: 'start' | 'end'): void {
    event.preventDefault();
    event.stopPropagation();
    (event.target as Element).setPointerCapture?.(event.pointerId);
    const origin = edge === 'start' ? block.startMin : block.endMin;
    this.drag = { block, edge, startY: event.clientY, origin };
    this.preview.set({ id: block.entry.id, edge, minutes: origin });
  }

  onPointerMove(event: PointerEvent): void {
    const d = this.drag;
    if (!d) return;
    let minutes = snap(d.origin + (event.clientY - d.startY) / PX);
    if (d.edge === 'start') {
      minutes = Math.min(Math.max(0, minutes), d.block.endMin - SNAP_MINUTES);
    } else {
      const nowMin = (this.now() - minuteToDate(this.day(), 0).getTime()) / 60000;
      const max = Math.min(dayMinutes(this.day()), nowMin);
      minutes = Math.max(d.block.startMin + SNAP_MINUTES, Math.min(minutes, max));
    }
    this.preview.set({ id: d.block.entry.id, edge: d.edge, minutes });
  }

  onPointerUp(): void {
    const d = this.drag;
    const p = this.preview();
    this.drag = null;
    if (!d || !p) return this.preview.set(null);
    if (p.minutes === d.origin) return this.preview.set(null);
    const at = minuteToDate(this.day(), p.minutes);
    void this.tasks
      .updateEntry(d.block.entry.id, d.edge === 'start' ? { start: at } : { end: at })
      .then(() => this.preview.set(null));
  }

  cancelDrag(): void {
    this.drag = null;
    this.preview.set(null);
  }

  save(entry: EntryView, change: EntryChange): void {
    void this.tasks.updateEntry(entry.id, change);
  }

  remove(entry: EntryView): void {
    if (confirm('Delete this time entry? This cannot be undone.')) {
      this.selectedId.set(null);
      void this.tasks.deleteEntry(entry.id);
    }
  }
}
