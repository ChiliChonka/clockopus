import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { SessionService } from './session.service';

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('SessionService', () => {
  let service: SessionService;
  let http: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting()],
    });
    service = TestBed.inject(SessionService);
    service.browserTimeZone = () => 'Europe/Berlin';
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => http.verify());

  it('loads config, sends the browser time zone on the first visit, then the user', async () => {
    const done = service.load();
    // Literals as the backend sends them (backend/src/app.rs and api.rs tests).
    http.expectOne('/api/v1/config').flush(JSON.parse('{"auth":"none"}'));
    await tick();
    http.expectOne('/api/v1/me/settings').flush(JSON.parse('{"timezone":null}'));
    await tick();
    const put = http.expectOne({ method: 'PUT', url: '/api/v1/me/settings' });
    expect(JSON.stringify(put.request.body)).toBe('{"timezone":"Europe/Berlin"}');
    expect(service.me()).toBeNull();
    put.flush(JSON.parse('{"timezone":"Europe/Berlin"}'));
    await tick();
    http.expectOne('/api/v1/me').flush(JSON.parse('{"id":"local","name":"Local user"}'));
    await done;

    expect(service.config()).toEqual({ auth: 'none' });
    expect(service.me()).toEqual({ id: 'local', name: 'Local user' });
    expect(service.error()).toBeNull();
  });

  it('keeps a time zone that is already set', async () => {
    const done = service.load();
    http.expectOne('/api/v1/config').flush({ auth: 'none' });
    await tick();
    http.expectOne('/api/v1/me/settings').flush({ timezone: 'America/New_York' });
    await tick();
    http.expectNone({ method: 'PUT', url: '/api/v1/me/settings' });
    http.expectOne('/api/v1/me').flush({ id: 'local', name: 'Local user' });
    await done;
    expect(service.me()?.id).toBe('local');
  });

  it('reports an unreachable server', async () => {
    const done = service.load();
    http.expectOne('/api/v1/config').flush('down', { status: 503, statusText: 'Unavailable' });
    await done;

    expect(service.me()).toBeNull();
    expect(service.error()).toContain('not reachable');
  });
});
