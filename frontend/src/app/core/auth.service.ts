import { Injectable, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { OAuthService } from 'angular-oauth2-oidc';

/** OIDC settings from `GET /api/v1/config`. */
export interface OidcConfig {
  issuer: string;
  client_id: string;
}

/**
 * Sign-in with OpenID Connect: Authorization Code flow with PKCE as a public client. The
 * backend checks every access token itself; this service only obtains and refreshes them.
 */
@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly oauth = inject(OAuthService);
  private readonly router = inject(Router);

  /** True once a sign-in is configured (auth mode `oidc`). */
  readonly enabled = signal(false);

  /**
   * Completes a sign-in that comes back from the provider, or starts one. Resolves to `true`
   * when there is a valid access token, `false` while the browser is redirected.
   */
  async signIn(config: OidcConfig): Promise<boolean> {
    this.enabled.set(true);
    this.oauth.configure({
      issuer: config.issuer,
      clientId: config.client_id,
      redirectUri: `${window.location.origin}/`,
      postLogoutRedirectUri: `${window.location.origin}/`,
      responseType: 'code',
      scope: 'openid profile email',
      // http is allowed for localhost only (development).
      requireHttps: 'remoteOnly',
      sessionChecksEnabled: false,
      showDebugInformation: false,
    });
    await this.oauth.loadDiscoveryDocumentAndTryLogin();
    if (this.oauth.hasValidAccessToken()) {
      // Keeps the session alive with the refresh token.
      this.oauth.setupAutomaticSilentRefresh();
      const target = returnTarget(this.oauth.state);
      if (target) {
        void this.router.navigateByUrl(target, { replaceUrl: true });
      } else if (new URLSearchParams(window.location.search).has('iss')) {
        // The provider's `iss` parameter (RFC 9207) stays in the address otherwise.
        void this.router.navigateByUrl('/', { replaceUrl: true });
      }
      return true;
    }
    this.login();
    return false;
  }

  /** Redirects to the provider; afterwards the current page opens again. */
  login(): void {
    // The library encodes the state itself.
    this.oauth.initCodeFlow(window.location.pathname + window.location.search);
  }

  logout(): void {
    this.oauth.logOut();
  }
}

/**
 * The page to return to after sign-in, from the OAuth state: a path inside this app only, never
 * another site (`//host`, `https:`, `javascript:`). The state may arrive encoded more than once.
 */
export function returnTarget(state: string | undefined): string | null {
  let target = state ?? '';
  for (let i = 0; i < 3 && /%[0-9A-Fa-f]{2}/.test(target); i++) {
    try {
      target = decodeURIComponent(target);
    } catch {
      return null;
    }
  }
  const inside = target.startsWith('/') && !target.startsWith('//') && !target.startsWith('/\\');
  return inside && target !== '/' ? target : null;
}
