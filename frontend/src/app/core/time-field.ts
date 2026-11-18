import { Component, computed, input, model } from '@angular/core';
import { parseHm } from './local-time';

/**
 * A time of day in 24-hour form without seconds. The browser's `<input type="time">` follows the
 * browser language (AM/PM in English) and cannot be forced to 24 hours, so this is a text field
 * that accepts `930`, `9:30`, `9.30` … and shows `09:30` after leaving it.
 */
@Component({
  selector: 'app-time-field',
  template: `
    <input
      type="text"
      inputmode="numeric"
      autocomplete="off"
      maxlength="5"
      placeholder="HH:MM"
      [attr.aria-label]="label()"
      [attr.aria-invalid]="invalid() || null"
      [class.invalid]="invalid()"
      [value]="value()"
      (input)="value.set(field.value)"
      (blur)="normalize(field)"
      #field
    />
  `,
  styles: `
    :host {
      display: inline-block;
    }
    input {
      width: 4.5em;
      border: 1px solid var(--border);
      border-radius: 8px;
      background: var(--surface);
      color: var(--text);
      padding: 4px 6px;
      font: inherit;
      font-variant-numeric: tabular-nums;
      text-align: center;
    }
    input.invalid {
      border-color: var(--danger);
    }
  `,
})
export class TimeField {
  /** The text in the field; `HH:MM` once valid and left. */
  readonly value = model('');
  readonly label = input('Time');

  readonly invalid = computed(() => this.value().trim() !== '' && parseHm(this.value()) === null);

  normalize(field: HTMLInputElement): void {
    const hm = parseHm(field.value);
    if (hm) {
      this.value.set(hm);
      field.value = hm;
    }
  }
}
