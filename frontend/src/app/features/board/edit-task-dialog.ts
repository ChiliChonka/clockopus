import {
  Component,
  ElementRef,
  effect,
  inject,
  input,
  linkedSignal,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { combine, localDate } from '../../core/local-time';
import { TimeField } from '../../core/time-field';
import { TasksService } from '../../core/tasks.service';
import { EntryView, TaskView } from '../../core/wire';
import { EntryChange, EntryRow } from '../entries/entry-row';

/** Splits `review #Team-A, x` into lower-case tag names, each once. */
export function parseTagInput(text: string): string[] {
  return text
    .split(/[\s,]+/)
    .map((t) => t.replace(/^#+/, '').toLowerCase())
    .filter((t, i, all) => t !== '' && all.indexOf(t) === i);
}

/** Edit dialog (docs/SPEC.md, 4.2): title, tags, link, notes, entries, add time. */
@Component({
  selector: 'app-edit-task-dialog',
  imports: [EntryRow, TimeField],
  template: `
    <dialog #dialog (close)="closed.emit()" aria-labelledby="edit-title">
      @if (task(); as task) {
        <header>
          <h2 id="edit-title">Edit task</h2>
          <button type="button" class="close" aria-label="Close" (click)="dialog.close()">✕</button>
        </header>
        <form class="task" (submit)="$event.preventDefault(); saveTask(task)">
          <label
            >Title <input type="text" [value]="title()" (input)="title.set(t.value)" #t
          /></label>
          <label>
            Tags
            <input
              type="text"
              [value]="tags()"
              (input)="tags.set(g.value)"
              placeholder="review team-a"
              #g
            />
          </label>
          <label>
            Link
            <input
              type="text"
              [value]="link()"
              (input)="link.set(l.value)"
              placeholder="https://…"
              #l
            />
          </label>
          <label class="notes">
            Notes
            <textarea
              rows="6"
              maxlength="20000"
              [value]="notes()"
              (input)="notes.set(n.value)"
              (keydown.control.enter)="saveTask(task)"
              (keydown.meta.enter)="saveTask(task)"
              placeholder="Anything worth keeping about this task. Ctrl+Enter saves."
              #n
            ></textarea>
          </label>
          <button type="submit" class="primary">Save task</button>
        </form>

        @if (tasks.error(); as error) {
          <p class="error" role="alert">{{ error }}</p>
        }

        <h3>Add time</h3>
        <form class="add" (submit)="$event.preventDefault(); addTime(task)">
          <label
            >Date <input type="date" [value]="addDate()" (input)="addDate.set(d.value)" #d
          /></label>
          <span class="field">From <app-time-field label="From" [(value)]="addFrom" /></span>
          <span class="field">To <app-time-field label="To" [(value)]="addTo" /></span>
          <label class="grow">
            Note <input type="text" [value]="addNote()" (input)="addNote.set(an.value)" #an />
          </label>
          <button type="submit">Add</button>
        </form>

        <h3>Time entries</h3>
        @for (entry of entries(); track entry.id) {
          <app-entry-row
            [entry]="entry"
            (save)="saveEntry(entry, $event)"
            (remove)="removeEntry(entry)"
          />
        } @empty {
          <p class="muted">No time on this task yet.</p>
        }
      }
    </dialog>
  `,
  styles: `
    dialog {
      width: min(760px, calc(100vw - 32px));
      max-height: calc(100vh - 48px);
      padding: 16px 20px 20px;
      border: 1px solid var(--border);
      border-radius: var(--radius);
      background: var(--bg);
      color: var(--text);
    }
    dialog::backdrop {
      background: rgb(0 0 0 / 0.4);
    }
    header {
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    h2 {
      margin: 0;
      font-size: 1.2rem;
    }
    h3 {
      margin: 20px 0 8px;
      font-size: 1rem;
    }
    .close {
      border: 0;
      background: none;
    }
    form {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      align-items: end;
    }
    label,
    .field {
      display: grid;
      gap: 2px;
      font-size: 0.85rem;
      color: var(--text-muted);
    }
    .task label,
    .grow {
      flex: 1 1 12rem;
    }
    .task label.notes {
      flex-basis: 100%;
    }
    textarea {
      resize: vertical;
      min-height: 6rem;
      font: inherit;
      line-height: 1.4;
    }
    input,
    textarea {
      border: 1px solid var(--border);
      border-radius: 8px;
      background: var(--surface);
      padding: 6px 8px;
      color: var(--text);
    }
    app-entry-row {
      display: block;
      padding: 6px 0;
      border-bottom: 1px solid var(--border);
    }
    .error {
      margin: 12px 0 0;
      color: var(--danger);
    }
  `,
})
export class EditTaskDialog {
  protected readonly tasks = inject(TasksService);
  readonly task = input<TaskView | null>(null);
  readonly closed = output<void>();
  private readonly dialog = viewChild.required<ElementRef<HTMLDialogElement>>('dialog');

  readonly title = linkedSignal(() => this.task()?.title ?? '');
  readonly tags = linkedSignal(() => (this.task()?.tags ?? []).join(' '));
  readonly link = linkedSignal(() => this.task()?.link ?? '');
  readonly notes = linkedSignal(() => this.task()?.notes ?? '');
  readonly entries = signal<EntryView[]>([]);

  readonly addDate = signal(localDate(new Date()));
  readonly addFrom = signal('');
  readonly addTo = signal('');
  readonly addNote = signal('');

  constructor() {
    // Open as a modal while a task is set.
    effect(() => {
      const el = this.dialog().nativeElement;
      if (this.task() && !el.open) {
        untracked(() => this.tasks.clearError());
        el.showModal();
      } else if (!this.task() && el.open) {
        el.close();
      }
    });
    // (Re)load the entries when the task changes or anything was saved.
    effect(() => {
      const task = this.task();
      this.tasks.revision();
      if (task) untracked(() => void this.loadEntries(task.id));
    });
  }

  private async loadEntries(taskId: string): Promise<void> {
    this.entries.set(await this.tasks.entries(taskId));
  }

  saveTask(task: TaskView): void {
    void this.tasks.update(task.id, {
      title: this.title(),
      tags: parseTagInput(this.tags()),
      link: this.link().trim(),
      // Sent every time; the server moves the “changed” date only when the text changed.
      notes: this.notes(),
    });
  }

  async addTime(task: TaskView): Promise<void> {
    const start = combine(this.addDate(), this.addFrom());
    const end = combine(this.addDate(), this.addTo());
    if (!start || !end) {
      this.tasks.error.set('Enter a date and both times (HH:MM, 24 hours).');
      return;
    }
    if (await this.tasks.addEntry(task.id, start, end, this.addNote())) {
      this.addFrom.set('');
      this.addTo.set('');
      this.addNote.set('');
    }
  }

  saveEntry(entry: EntryView, change: EntryChange): void {
    void this.tasks.updateEntry(entry.id, change);
  }

  removeEntry(entry: EntryView): void {
    if (confirm('Delete this time entry? This cannot be undone.')) {
      void this.tasks.deleteEntry(entry.id);
    }
  }
}
