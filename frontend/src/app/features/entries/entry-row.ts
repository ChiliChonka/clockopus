import { Component, computed, input, linkedSignal, output, signal } from '@angular/core';
import { formatDuration } from '../../core/format';
import { combine, localDate, localTime, splitLocal } from '../../core/local-time';
import { TimeField } from '../../core/time-field';
import { EntryView } from '../../core/wire';

export interface EntryChange {
  start?: Date;
  end?: Date;
  note?: string;
}

/** One time entry, editable in place: start, end, note; history on demand (R6). */
@Component({
  selector: 'app-entry-row',
  imports: [TimeField],
  template: `
    <form class="row" (submit)="$event.preventDefault(); submit()">
      <span class="when">
        <input
          type="date"
          aria-label="Start date"
          [value]="startDate()"
          (input)="startDate.set(sd.value)"
          #sd
        />
        <app-time-field label="Start time" [(value)]="startTime" />
      </span>
      <span aria-hidden="true">–</span>
      @if (entry().end) {
        <span class="when">
          <input
            type="date"
            aria-label="End date"
            [value]="endDate()"
            (input)="endDate.set(ed.value)"
            #ed
          />
          <app-time-field label="End time" [(value)]="endTime" />
        </span>
      } @else {
        <span class="running">running</span>
      }
      <span class="duration">{{ duration() }}</span>
      <label class="note">
        <span class="sr-only">Note</span>
        <input
          type="text"
          placeholder="Note"
          [value]="note()"
          (input)="note.set(noteInput.value)"
          #noteInput
        />
      </label>
      <button type="submit" class="primary" [disabled]="!changed()">Save</button>
      <button type="button" class="danger" (click)="remove.emit()">Delete</button>
      @if (entry().history.length) {
        <button
          type="button"
          class="link"
          (click)="showHistory.set(!showHistory())"
          [attr.aria-expanded]="showHistory()"
        >
          {{ entry().history.length }} {{ entry().history.length === 1 ? 'change' : 'changes' }}
        </button>
      }
    </form>
    @if (invalid(); as message) {
      <p class="error">{{ message }}</p>
    }
    @if (showHistory()) {
      <ul class="history">
        @for (c of entry().history; track $index) {
          <li>
            {{ when(c.changed_at) }}: was {{ when(c.start) }} –
            {{ c.end ? when(c.end) : 'running' }} ({{
              c.reason === 'forgotten-timer' ? 'forgotten timer' : 'edit'
            }})
          </li>
        }
      </ul>
    }
  `,
  styles: `
    .row {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      align-items: center;
    }
    .when {
      display: inline-flex;
      gap: 4px;
    }
    input {
      border: 1px solid var(--border);
      border-radius: 8px;
      background: var(--surface);
      padding: 4px 6px;
    }
    .note {
      flex: 1;
      min-width: 8rem;
    }
    .note input {
      width: 100%;
    }
    .duration,
    .running {
      min-width: 3.5rem;
      font-variant-numeric: tabular-nums;
      color: var(--text-muted);
    }
    .running {
      color: var(--running);
    }
    .danger {
      color: var(--danger);
    }
    .link {
      border: 0;
      background: none;
      color: var(--accent);
      text-decoration: underline;
    }
    .error {
      margin: 4px 0 0;
      color: var(--danger);
    }
    .history {
      margin: 4px 0 0;
      padding-left: 20px;
      color: var(--text-muted);
      font-size: 0.85rem;
    }
  `,
})
export class EntryRow {
  readonly entry = input.required<EntryView>();
  readonly save = output<EntryChange>();
  readonly remove = output<void>();

  private readonly startParts = computed(() => splitLocal(this.entry().start));
  private readonly endParts = computed(() =>
    this.entry().end ? splitLocal(this.entry().end!) : { date: '', time: '' },
  );
  readonly startDate = linkedSignal(() => this.startParts().date);
  readonly startTime = linkedSignal(() => this.startParts().time);
  readonly endDate = linkedSignal(() => this.endParts().date);
  readonly endTime = linkedSignal(() => this.endParts().time);
  readonly note = linkedSignal(() => this.entry().note ?? '');
  readonly showHistory = signal(false);
  readonly invalid = signal<string | null>(null);

  private readonly start = computed(() => combine(this.startDate(), this.startTime()));
  private readonly end = computed(() =>
    this.entry().end ? combine(this.endDate(), this.endTime()) : new Date(),
  );

  readonly changed = computed(() => {
    const original = new Date(this.entry().start).getTime();
    const startChanged = this.start()?.getTime() !== original;
    const endChanged =
      this.entry().end !== null && this.end()?.getTime() !== new Date(this.entry().end!).getTime();
    return startChanged || endChanged || this.note() !== (this.entry().note ?? '');
  });

  readonly duration = computed(() => {
    const start = this.start();
    const end = this.end();
    return start && end ? formatDuration((end.getTime() - start.getTime()) / 1000) : '';
  });

  when(iso: string): string {
    const d = new Date(iso);
    return `${localDate(d)} ${localTime(d)}`;
  }

  submit(): void {
    const change: EntryChange = {};
    const start = this.start();
    if (!start) return this.invalid.set('The start is not a valid date and time (HH:MM).');
    if (start.getTime() !== new Date(this.entry().start).getTime()) change.start = start;
    if (this.entry().end) {
      const end = this.end();
      if (!end) return this.invalid.set('The end is not a valid date and time (HH:MM).');
      if (end <= start) return this.invalid.set('The end must be after the start.');
      if (end.getTime() !== new Date(this.entry().end!).getTime()) change.end = end;
    }
    if (this.note() !== (this.entry().note ?? '')) change.note = this.note();
    this.invalid.set(null);
    this.save.emit(change);
  }
}
