import { Component, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { SessionService } from './core/session.service';

@Component({
  imports: [RouterOutlet],
  selector: 'app-root',
  template: `
    <header>
      <a class="brand" href="/">
        <img src="logo.svg" alt="" width="36" height="36" />
        <span>clockopus</span>
      </a>
      @if (session.me(); as me) {
        <span class="user">{{ me.name }}</span>
      }
    </header>
    @if (session.error(); as error) {
      <p class="error" role="alert">{{ error }}</p>
    }
    <main>
      <router-outlet />
    </main>
  `,
  styles: `
    header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: var(--gap);
      padding: 8px 16px;
      background: var(--surface);
      border-bottom: 1px solid var(--border);
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 8px;
      color: var(--text);
      text-decoration: none;
      font-weight: 700;
      font-size: 1.15rem;
    }
    .user {
      color: var(--text-muted);
    }
    .error {
      margin: 16px;
      padding: 12px 16px;
      border: 1px solid var(--danger);
      border-radius: var(--radius);
      color: var(--danger);
    }
    main {
      max-width: 1100px;
      margin: 0 auto;
      padding: 16px;
    }
  `,
})
export class App {
  protected readonly session = inject(SessionService);

  constructor() {
    void this.session.load();
  }
}
