import { Component } from '@angular/core';

/** The board of task cards. Tasks and the timer arrive with milestone M2. */
@Component({
  selector: 'app-board',
  template: `
    <section class="empty">
      <img src="logo.svg" alt="" width="120" height="120" />
      <h2>No tasks yet</h2>
      <p>Your tasks will appear here as cards with start and stop buttons.</p>
    </section>
  `,
  styles: `
    .empty {
      display: grid;
      justify-items: center;
      gap: 8px;
      padding: 64px 16px;
      text-align: center;
      color: var(--text-muted);
    }
    h2 {
      margin: 8px 0 0;
      color: var(--text);
      font-size: 1.25rem;
    }
    p {
      margin: 0;
      max-width: 32rem;
    }
  `,
})
export class Board {}
