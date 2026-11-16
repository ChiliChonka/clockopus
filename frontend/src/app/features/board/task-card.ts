import { Component, computed, input, output } from '@angular/core';
import { formatHm, formatHms } from '../../core/format';
import { TaskView } from '../../core/wire';

/** One task on the board: title, tags, time today, start/stop and a small menu. */
@Component({
  selector: 'app-task-card',
  template: `
    <article
      class="card"
      [class.is-running]="running()"
      [class.is-done]="task().status === 'done'"
      [attr.aria-label]="task().title"
    >
      <header>
        <h3>{{ task().title }}</h3>
        @if (task().pinned) {
          <span class="pin" title="Pinned" aria-label="Pinned">📌</span>
        }
        @if (task().link; as link) {
          <a [href]="link" target="_blank" rel="noopener noreferrer" title="Open link">↗</a>
        }
      </header>
      @if (task().tags.length) {
        <p class="tags">
          @for (tag of task().tags; track tag) {
            <span class="chip">#{{ tag }}</span>
          }
        </p>
      }
      <p class="time">
        @if (running()) {
          <strong class="elapsed">{{ elapsed() }}</strong>
        }
        <span class="muted">today {{ today() }}</span>
      </p>
      <footer>
        <div class="buttons">
          @if (running()) {
            <button type="button" class="running" (click)="stop.emit()">Stop</button>
          } @else {
            <button type="button" class="primary" (click)="start.emit()">Start</button>
          }
          <button type="button" (click)="edit.emit()">Edit</button>
        </div>
        <details #menu>
          <summary aria-label="More actions">⋯</summary>
          <div class="menu" (click)="menu.open = false">
            <button type="button" (click)="pin.emit(!task().pinned)">
              {{ task().pinned ? 'Unpin' : 'Pin' }}
            </button>
            @if (task().status === 'active') {
              <button type="button" (click)="done.emit()">Mark done</button>
            }
            <button type="button" class="danger" (click)="remove.emit()">Delete</button>
          </div>
        </details>
      </footer>
    </article>
  `,
  styles: `
    .card {
      display: flex;
      flex-direction: column;
      gap: 8px;
      height: 100%;
      padding: 12px 14px;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: var(--radius);
    }
    .is-running {
      border: 2px solid var(--running);
    }
    .is-done h3 {
      text-decoration: line-through;
      color: var(--text-muted);
    }
    header {
      display: flex;
      align-items: start;
      gap: 6px;
    }
    h3 {
      flex: 1;
      margin: 0;
      font-size: 1rem;
      overflow-wrap: anywhere;
    }
    header a {
      color: var(--accent);
      text-decoration: none;
    }
    p {
      margin: 0;
    }
    .tags {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
    }
    .time {
      display: flex;
      gap: 8px;
      align-items: baseline;
      margin-top: auto;
      font-variant-numeric: tabular-nums;
    }
    .elapsed {
      font-size: 1.25rem;
    }
    .buttons {
      display: flex;
      gap: 6px;
    }
    footer {
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    details {
      position: relative;
    }
    summary {
      list-style: none;
      cursor: pointer;
      padding: 4px 10px;
      border-radius: 8px;
    }
    summary::-webkit-details-marker {
      display: none;
    }
    .menu {
      position: absolute;
      right: 0;
      z-index: 2;
      display: grid;
      min-width: 9rem;
      padding: 4px;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 8px;
      box-shadow: 0 4px 16px rgb(0 0 0 / 0.15);
    }
    .menu button {
      border: 0;
      text-align: left;
    }
    .danger {
      color: var(--danger);
    }
  `,
})
export class TaskCard {
  readonly task = input.required<TaskView>();
  /** Current time in ms; the board ticks it every second. */
  readonly now = input.required<number>();
  /** When the task list was loaded (ms); live times add `now - loadedAt`. */
  readonly loadedAt = input.required<number>();

  readonly start = output<void>();
  readonly stop = output<void>();
  readonly pin = output<boolean>();
  readonly done = output<void>();
  readonly remove = output<void>();
  readonly edit = output<void>();

  readonly running = computed(() => this.task().running_since !== null);

  readonly elapsed = computed(() => {
    const since = this.task().running_since;
    return since ? formatHms((this.now() - Date.parse(since)) / 1000) : '';
  });

  readonly today = computed(() => {
    const live = this.running() ? Math.max(0, (this.now() - this.loadedAt()) / 1000) : 0;
    return formatHm(this.task().today_seconds + live);
  });
}
