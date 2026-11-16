import { HttpClient, provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom } from 'rxjs';
import { reauthOn401 } from './auth.interceptor';
import { AuthService } from './auth.service';

describe('reauthOn401', () => {
  let http: HttpClient;
  let backend: HttpTestingController;
  const auth = { enabled: signal(true), login: vi.fn() };

  beforeEach(() => {
    auth.enabled.set(true);
    auth.login.mockClear();
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([reauthOn401])),
        provideHttpClientTesting(),
        { provide: AuthService, useValue: auth },
      ],
    });
    http = TestBed.inject(HttpClient);
    backend = TestBed.inject(HttpTestingController);
  });

  afterEach(() => backend.verify());

  async function fail(url: string, status: number) {
    const done = firstValueFrom(http.get(url)).catch((e) => e);
    backend.expectOne(url).flush({}, { status, statusText: 'x' });
    return done;
  }

  it('starts a new sign-in when the API answers 401', async () => {
    const e = await fail('/api/v1/tasks', 401);
    expect(e.status).toBe(401);
    expect(auth.login).toHaveBeenCalledTimes(1);
  });

  it('leaves other errors, other URLs and the mode without sign-in alone', async () => {
    await fail('/api/v1/tasks', 400);
    await fail('/elsewhere', 401);
    auth.enabled.set(false);
    await fail('/api/v1/tasks', 401);
    expect(auth.login).not.toHaveBeenCalled();
  });
});
