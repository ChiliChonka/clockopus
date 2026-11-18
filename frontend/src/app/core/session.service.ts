import { HttpClient } from '@angular/common/http';
import { Injectable, inject, signal } from '@angular/core';
import { firstValueFrom } from 'rxjs';
import { AuthService } from './auth.service';
import { Settings, SettingsPatch } from './wire';

/** Mirrors `PublicConfig` in backend/src/app.rs. */
export type PublicConfig = { version?: string } & (
  { auth: 'none' } | { auth: 'oidc'; issuer: string; client_id: string }
);

/** Mirrors `Me` in backend/src/app.rs. */
export interface Me {
  id: string;
  name: string;
}

/** Runtime configuration and the signed-in user, loaded once at startup. */
@Injectable({ providedIn: 'root' })
export class SessionService {
  private readonly http = inject(HttpClient);
  private readonly auth = inject(AuthService);

  readonly config = signal<PublicConfig | null>(null);
  /** Set last: once there is a user, the settings (time zone) are in place too. */
  readonly me = signal<Me | null>(null);
  readonly settings = signal<Settings | null>(null);
  readonly error = signal<string | null>(null);

  /** The browser's IANA time zone; replaceable in tests. */
  browserTimeZone: () => string = () => Intl.DateTimeFormat().resolvedOptions().timeZone;

  private signedIn = false;

  async load(): Promise<void> {
    try {
      const config = await firstValueFrom(this.http.get<PublicConfig>('/api/v1/config'));
      this.config.set(config);
      if (config.auth === 'oidc' && !this.signedIn) {
        // `false`: the browser is on its way to the sign-in page.
        if (!(await this.auth.signIn(config))) return;
        this.signedIn = true;
      }
      await this.ensureTimeZone();
      this.me.set(await firstValueFrom(this.http.get<Me>('/api/v1/me')));
      this.error.set(null);
    } catch {
      this.error.set('The clockopus server is not reachable. Please try again in a moment.');
    }
  }

  /** "Today" is computed in the user's time zone; the first visit sends the browser's. */
  private async ensureTimeZone(): Promise<void> {
    const settings = await firstValueFrom(this.http.get<Settings>('/api/v1/me/settings'));
    this.settings.set(settings);
    if (settings.timezone) return;
    const timezone = this.browserTimeZone();
    if (!timezone) return;
    this.settings.set(
      await firstValueFrom(this.http.put<Settings>('/api/v1/me/settings', { timezone })),
    );
  }

  /** Saves the given settings; throws the HTTP error so the form can show it. */
  async saveSettings(patch: SettingsPatch): Promise<void> {
    this.settings.set(await firstValueFrom(this.http.put<Settings>('/api/v1/me/settings', patch)));
  }
}
