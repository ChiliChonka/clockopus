import { Component, computed, input, linkedSignal, output, signal } from '@angular/core';
import { formatHm } from '../../core/format';
import { fromLocalInput, localDate, localTime, toLocalInput } from '../../core/local-time';
import { EntryView } from '../../core/wire';

export interface EntryChange {
  start?: Date;
  end?: Date;
  note?: string;
}

/** One time entry, editable in place: start, end, note; history on demand (R6). */
@Component({
  selector: 'app-entry-row',
  template: `
    <form class="row" (submit)="$event.preventDefault(); submit()">
      <label>
        <span class="sr-only">Start</span>
        <input
          type="datetime-local"
          [value]="start()"
          (input)="start.set(startInput.value)"
          #startInput
        />
      </label>
      <span aria-hidden="true">–</span>
      @if (entry().end) {
        <label>
          <span class="sr-only">End</span>
          <input
            type="datetime-local"
            [value]="end()"
            (input)="end.set(endInput.value)"
            #endInput
          />
        </label>
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

  readonly start = linkedSignal(() => toLocalInput(this.entry().start));
  readonly end = linkedSignal(() => (this.entry().end ? toLocalInput(this.entry().end!) : ''));
  readonly note = linkedSignal(() => this.entry().note ?? '');
  readonly showHistory = signal(false);
  readonly invalid = signal<string | null>(null);

  readonly changed = computed(
    () =>
      this.start() !== toLocalInput(this.entry().start) ||
      (this.entry().end !== null && this.end() !== toLocalInput(this.entry().end!)) ||
      this.note() !== (this.entry().note ?? ''),
  );

  readonly duration = computed(() => {
    const start = fromLocalInput(this.start());
    const end = this.entry().end ? fromLocalInput(this.end()) : new Date();
    return start && end ? formatHm((end.getTime() - start.getTime()) / 1000) : '';
  });

  when(iso: string): string {
    const d = new Date(iso);
    return `${localDate(d)} ${localTime(d)}`;
  }

  submit(): void {
    const change: EntryChange = {};
    const start = fromLocalInput(this.start());
    if (!start) return this.invalid.set('The start is not a valid time.');
    if (this.start() !== toLocalInput(this.entry().start)) change.start = start;
    if (this.entry().end) {
      const end = fromLocalInput(this.end());
      if (!end) return this.invalid.set('The end is not a valid time.');
      if (end <= start) return this.invalid.set('The end must be after the start.');
      if (this.end() !== toLocalInput(this.entry().end!)) change.end = end;
    }
    if (this.note() !== (this.entry().note ?? '')) change.note = this.note();
    this.invalid.set(null);
    this.save.emit(change);
  }
}
