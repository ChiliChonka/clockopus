import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { OAuthService } from 'angular-oauth2-oidc';
import { AuthService, returnTarget } from './auth.service';

describe('AuthService', () => {
  let service: AuthService;
  const oauth = {
    configure: vi.fn(),
    loadDiscoveryDocumentAndTryLogin: vi.fn().mockResolvedValue(true),
    hasValidAccessToken: vi.fn(),
    setupAutomaticSilentRefresh: vi.fn(),
    initCodeFlow: vi.fn(),
    logOut: vi.fn(),
    state: '' as string | undefined,
  };
  const router = { navigateByUrl: vi.fn() };
  const config = { issuer: 'https://auth.example.com/realms/family', client_id: 'clockopus' };

  beforeEach(() => {
    vi.clearAllMocks();
    oauth.state = '';
    TestBed.configureTestingModule({
      providers: [
        { provide: OAuthService, useValue: oauth },
        { provide: Router, useValue: router },
      ],
    });
    service = TestBed.inject(AuthService);
  });

  it('configures the code flow with PKCE for this origin', async () => {
    oauth.hasValidAccessToken.mockReturnValue(true);
    await service.signIn(config);
    expect(oauth.configure).toHaveBeenCalledWith(
      expect.objectContaining({
        issuer: 'https://auth.example.com/realms/family',
        clientId: 'clockopus',
        responseType: 'code',
        redirectUri: `${window.location.origin}/`,
        scope: 'openid profile email',
        requireHttps: 'remoteOnly',
      }),
    );
    expect(service.enabled()).toBe(true);
  });

  it('returns true with a token, keeps it fresh and goes back to the page of before', async () => {
    oauth.hasValidAccessToken.mockReturnValue(true);
    oauth.state = encodeURIComponent('/week/2026-11-18?x=1');
    expect(await service.signIn(config)).toBe(true);
    expect(oauth.setupAutomaticSilentRefresh).toHaveBeenCalledTimes(1);
    expect(router.navigateByUrl).toHaveBeenCalledWith('/week/2026-11-18?x=1', {
      replaceUrl: true,
    });
    expect(oauth.initCodeFlow).not.toHaveBeenCalled();
  });

  it('never follows a return target to another site', async () => {
    oauth.hasValidAccessToken.mockReturnValue(true);
    let navigated = 0;
    for (const state of [
      '//evil.example.com/x',
      'https://evil.example.com',
      'javascript:alert(1)',
    ]) {
      oauth.state = encodeURIComponent(state);
      await service.signIn(config);
      navigated += router.navigateByUrl.mock.calls.length;
    }
    expect(navigated).toBe(0);
  });

  it('redirects to the provider without a token, remembering the page', async () => {
    oauth.hasValidAccessToken.mockReturnValue(false);
    expect(await service.signIn(config)).toBe(false);
    const here = window.location.pathname + window.location.search;
    // Raw: the library encodes the state itself.
    expect(oauth.initCodeFlow).toHaveBeenCalledWith(here);
  });

  it('signs out at the provider', () => {
    service.logout();
    expect(oauth.logOut).toHaveBeenCalledTimes(1);
  });

  it('reads the return target however often it was encoded', () => {
    // As seen with Keycloak: the path arrived encoded three times.
    expect(returnTarget('%25252Fweek%25252F2026-11-18')).toBe('/week/2026-11-18');
    expect(returnTarget('%2Fweek%2F2026-11-18')).toBe('/week/2026-11-18');
    expect(returnTarget('/day/2026-11-10')).toBe('/day/2026-11-10');
    let refused = 0;
    for (const state of [
      undefined,
      '',
      '/',
      '//evil.example.com',
      '%2F%2Fevil.example.com',
      '/\\evil',
      'https://evil.example.com',
      '%E0%A4%A',
    ]) {
      expect(returnTarget(state)).toBeNull();
      refused++;
    }
    expect(refused).toBe(8);
  });
});
