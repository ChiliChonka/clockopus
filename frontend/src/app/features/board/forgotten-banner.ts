import { Component, computed, input, output, signal } from '@angular/core';
import { formatHuman } from '../../core/format';
import { fromLocalInput, relativeDayTime, toLocalInput } from '../../core/local-time';
import { Forgotten } from '../../core/wire';

/** R8: asks about a forgotten timer; never changes data by itself (docs/SPEC.md, 3.3). */
@Component({
  selector: 'app-forgotten-banner',
  template: `
    <section class="banner" role="alert" aria-labelledby="forgotten-text">
      <p id="forgotten-text">
        Your timer for <strong>{{ forgotten().task_title }}</strong> has been running since
        <strong>{{ since() }}</strong> ({{ duration() }}).
      </p>
      <div class="actions">
        @if (suggested(); as end) {
          <button type="button" class="primary" (click)="stopAt.emit(end.date)">
            Stop at {{ end.label }}
          </button>
        }
        @if (picking()) {
          <form (submit)="$event.preventDefault(); savePicked()">
            <label>
              End
              <input
                type="datetime-local"
                [value]="picked()"
                [min]="minInput()"
                [max]="maxInput()"
                (input)="picked.set(pickedInput.value)"
                #pickedInput
              />
            </label>
            <button type="submit" class="primary">Save</button>
            <button type="button" (click)="picking.set(false)">Cancel</button>
          </form>
        } @else {
          <button type="button" (click)="startPicking()">Pick end time…</button>
        }
        <button type="button" (click)="keep.emit()">Keep running</button>
      </div>
      @if (pickError(); as error) {
        <p class="error">{{ error }}</p>
      }
    </section>
  `,
  styles: `
    .banner {
      display: grid;
      gap: 10px;
      padding: 12px 16px;
      border: 2px solid var(--running);
      border-radius: var(--radius);
      background: var(--surface);
    }
    p {
      margin: 0;
    }
    .actions,
    form,
    label {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      align-items: center;
    }
    input {
      border: 1px solid var(--border);
      border-radius: 8px;
      background: var(--surface);
      padding: 5px 8px;
    }
    .error {
      color: var(--danger);
    }
  `,
})
export class ForgottenBanner {
  readonly forgotten = input.required<Forgotten>();
  /** Current time in ms. */
  readonly now = input.required<number>();
  readonly stopAt = output<Date>();
  readonly keep = output<void>();

  readonly picking = signal(false);
  readonly picked = signal('');
  readonly pickError = signal<string | null>(null);

  readonly since = computed(() => relativeDayTime(this.forgotten().start, new Date(this.now())));
  readonly duration = computed(() => formatHuman(this.forgotten().running_seconds));
  readonly suggested = computed(() => {
    const end = this.forgotten().suggested_end;
    return end ? { date: new Date(end), label: relativeDayTime(end, new Date(this.now())) } : null;
  });
  readonly minInput = computed(() => toLocalInput(this.forgotten().start));
  readonly maxInput = computed(() => toLocalInput(new Date(this.now()).toISOString()));

  startPicking(): void {
    const initial = this.forgotten().suggested_end ?? new Date(this.now()).toISOString();
    this.picked.set(toLocalInput(initial));
    this.pickError.set(null);
    this.picking.set(true);
  }

  savePicked(): void {
    const end = fromLocalInput(this.picked());
    const start = new Date(this.forgotten().start).getTime();
    if (!end || end.getTime() <= start || end.getTime() > this.now()) {
      this.pickError.set('Pick a time after the start and not in the future.');
      return;
    }
    this.pickError.set(null);
    this.picking.set(false);
    this.stopAt.emit(end);
  }
}
