import { HttpClient } from '@angular/common/http';
import { Injectable, inject, signal } from '@angular/core';
import { firstValueFrom } from 'rxjs';

/** Mirrors `PublicConfig` in backend/src/app.rs. */
export type PublicConfig =
  | { auth: 'none' }
  | { auth: 'oidc'; issuer: string; client_id: string };

/** Mirrors `Me` in backend/src/app.rs. */
export interface Me {
  id: string;
  name: string;
}

/** Runtime configuration and the signed-in user, loaded once at startup. */
@Injectable({ providedIn: 'root' })
export class SessionService {
  private readonly http = inject(HttpClient);

  readonly config = signal<PublicConfig | null>(null);
  readonly me = signal<Me | null>(null);
  readonly error = signal<string | null>(null);

  async load(): Promise<void> {
    try {
      this.config.set(await firstValueFrom(this.http.get<PublicConfig>('/api/v1/config')));
      this.me.set(await firstValueFrom(this.http.get<Me>('/api/v1/me')));
      this.error.set(null);
    } catch {
      this.error.set('The clockopus server is not reachable. Please try again in a moment.');
    }
  }
}
