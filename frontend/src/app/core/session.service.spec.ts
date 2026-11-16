import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { SessionService } from './session.service';

describe('SessionService', () => {
  let service: SessionService;
  let http: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting()],
    });
    service = TestBed.inject(SessionService);
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => http.verify());

  it('loads config and the current user', async () => {
    const done = service.load();
    // Literals as the backend sends them (backend/src/app.rs tests).
    http.expectOne('/api/v1/config').flush(JSON.parse('{"auth":"none"}'));
    await Promise.resolve();
    http.expectOne('/api/v1/me').flush(JSON.parse('{"id":"local","name":"Local user"}'));
    await done;

    expect(service.config()).toEqual({ auth: 'none' });
    expect(service.me()).toEqual({ id: 'local', name: 'Local user' });
    expect(service.error()).toBeNull();
  });

  it('reports an unreachable server', async () => {
    const done = service.load();
    http.expectOne('/api/v1/config').flush('down', { status: 503, statusText: 'Unavailable' });
    await done;

    expect(service.me()).toBeNull();
    expect(service.error()).toContain('not reachable');
  });
});
