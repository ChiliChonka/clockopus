import {
  Component,
  ElementRef,
  computed,
  input,
  model,
  output,
  signal,
  viewChild,
} from '@angular/core';
import {
  appendTag,
  completeTag,
  parseQuickAdd,
  suggestTags,
  tagTokenAt,
} from '../../core/quick-add';
import { TagView } from '../../core/wire';

export interface QuickAddSubmit {
  text: string;
  start: boolean;
}

/**
 * The quick-add field (docs/SPEC.md, section 4.1): `Enter` creates and starts, `Alt+Enter` only
 * creates, `Tab` completes the `#tag` under the caret, the bar below inserts frequent tags.
 */
@Component({
  selector: 'app-quick-add-field',
  template: `
    <form class="field" (submit)="$event.preventDefault(); send(true)">
      <label for="quick-add" class="sr-only">New task</label>
      <input
        #input
        id="quick-add"
        type="text"
        autocomplete="off"
        placeholder="What are you working on? Add #tags — Enter starts it"
        [value]="text()"
        (input)="onInput(input)"
        (keydown)="onKeydown($event)"
        (keyup)="caret.set(input.selectionStart ?? 0)"
        (click)="caret.set(input.selectionStart ?? 0)"
        (blur)="dismissed.set(true)"
        (focus)="dismissed.set(false)"
        aria-describedby="quick-add-hint"
        [attr.aria-expanded]="suggestions().length > 0"
        aria-controls="quick-add-suggestions"
      />
      <button type="submit" class="primary">Start</button>
      @if (suggestions().length) {
        <ul id="quick-add-suggestions" class="suggestions" role="listbox">
          @for (tag of suggestions(); track tag; let first = $first) {
            <li role="option" [attr.aria-selected]="first">
              <button type="button" (mousedown)="$event.preventDefault()" (click)="complete(tag)">
                #{{ tag }}
              </button>
            </li>
          }
        </ul>
      }
    </form>
    <p id="quick-add-hint" class="hint muted">
      Enter: create and start · Alt+Enter: only create · Tab: complete tag
    </p>
    @if (quickTags().length) {
      <div class="quick-tags" aria-label="Frequent tags">
        @for (tag of quickTags(); track tag.name) {
          <button type="button" class="chip" (click)="insert(tag.name)">#{{ tag.name }}</button>
        }
      </div>
    }
  `,
  styles: `
    .field {
      position: relative;
      display: flex;
      gap: 8px;
    }
    .suggestions {
      position: absolute;
      top: 100%;
      left: 0;
      z-index: 3;
      margin: 4px 0 0;
      padding: 4px;
      list-style: none;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 8px;
      box-shadow: 0 4px 16px rgb(0 0 0 / 0.15);
    }
    .suggestions button {
      width: 100%;
      border: 0;
      text-align: left;
    }
    [aria-selected='true'] button {
      background: var(--bg);
    }
    .hint {
      margin: 4px 0 8px;
      font-size: 0.8rem;
    }
    .quick-tags {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }
  `,
})
export class QuickAddField {
  readonly text = model('');
  readonly knownTags = input<TagView[]>([]);
  readonly quickTags = input<TagView[]>([]);
  readonly submitted = output<QuickAddSubmit>();

  readonly caret = signal(0);
  readonly dismissed = signal(false);
  private readonly inputEl = viewChild.required<ElementRef<HTMLInputElement>>('input');

  private readonly token = computed(() => tagTokenAt(this.text(), this.caret()));

  readonly suggestions = computed(() => {
    const token = this.token();
    if (!token || this.dismissed()) return [];
    const rest = this.text().slice(0, token.start) + this.text().slice(token.end);
    return suggestTags(
      this.knownTags().map((t) => t.name),
      token.prefix,
      parseQuickAdd(rest).tags,
    );
  });

  focus(): void {
    this.inputEl().nativeElement.focus();
  }

  onInput(el: HTMLInputElement): void {
    this.text.set(el.value);
    this.caret.set(el.selectionStart ?? el.value.length);
    this.dismissed.set(false);
  }

  onKeydown(event: KeyboardEvent): void {
    const first = this.suggestions()[0];
    if (event.key === 'Tab' && first) {
      event.preventDefault();
      this.complete(first);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      this.send(!event.altKey);
    } else if (event.key === 'Escape') {
      this.dismissed.set(true);
    }
  }

  complete(tag: string): void {
    const token = this.token();
    if (!token) return;
    const next = completeTag(this.text(), token, tag);
    this.setText(next.text, next.caret);
  }

  insert(tag: string): void {
    const next = appendTag(this.text(), tag);
    this.setText(next, next.length);
    this.dismissed.set(true);
    this.focus();
  }

  send(start: boolean): void {
    if (!this.text().trim()) return;
    this.submitted.emit({ text: this.text(), start });
  }

  private setText(text: string, caret: number): void {
    this.text.set(text);
    this.caret.set(caret);
    const el = this.inputEl().nativeElement;
    el.value = text;
    el.setSelectionRange(caret, caret);
  }
}
