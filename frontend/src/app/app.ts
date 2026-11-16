import { Component, inject } from '@angular/core';
import { RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { SessionService } from './core/session.service';

@Component({
  imports: [RouterOutlet, RouterLink, RouterLinkActive],
  selector: 'app-root',
  template: `
    <header>
      <a class="brand" routerLink="/">
        <img src="logo.svg" alt="" width="36" height="36" />
        <span>clockopus</span>
      </a>
      <nav aria-label="Views">
        <a
          routerLink="/"
          routerLinkActive="active"
          [routerLinkActiveOptions]="{ exact: true }"
          ariaCurrentWhenActive="page"
          >Board</a
        >
        <a routerLink="/day" routerLinkActive="active" ariaCurrentWhenActive="page">Day</a>
        <a routerLink="/week" routerLinkActive="active" ariaCurrentWhenActive="page">Week</a>
        <a routerLink="/month" routerLinkActive="active" ariaCurrentWhenActive="page">Month</a>
        <a
          routerLink="/settings"
          routerLinkActive="active"
          ariaCurrentWhenActive="page"
          aria-label="Settings"
          title="Settings"
          >⚙</a
        >
      </nav>
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
      white-space: nowrap;
    }
    @media (max-width: 480px) {
      .user {
        display: none;
      }
    }
    nav {
      display: flex;
      gap: 4px;
      margin-right: auto;
      min-width: 0;
      overflow-x: auto;
    }
    @media (max-width: 480px) {
      .brand span {
        display: none;
      }
    }
    nav a {
      padding: 4px 10px;
      border-radius: 8px;
      color: var(--text-muted);
      text-decoration: none;
    }
    nav a.active {
      background: var(--bg);
      color: var(--text);
      font-weight: 600;
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
