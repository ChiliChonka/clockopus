import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { AccountService, filenameFrom } from './account.service';

describe('AccountService', () => {
  let service: AccountService;
  let http: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting()],
    });
    service = TestBed.inject(AccountService);
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => http.verify());

  it('reads the file name from Content-Disposition', () => {
    expect(filenameFrom('attachment; filename="clockopus-2026-11-10-2026-11-10.csv"', 'x')).toBe(
      'clockopus-2026-11-10-2026-11-10.csv',
    );
    expect(filenameFrom(null, 'fallback.csv')).toBe('fallback.csv');
  });

  it('asks for a report of a range', async () => {
    const done = service.report('2026-11-16', '2026-11-22');
    http.expectOne('/api/v1/reports?from=2026-11-16&to=2026-11-22').flush({ total_seconds: 0 });
    expect((await done).total_seconds).toBe(0);
  });

  it('downloads the CSV with range and rounding and saves it under the server name', async () => {
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    URL.createObjectURL = vi.fn(() => 'blob:x');
    URL.revokeObjectURL = vi.fn();
    const done = service.downloadCsv('2026-11-01', '2026-11-30', 15);
    const req = http.expectOne('/api/v1/export.csv?from=2026-11-01&to=2026-11-30&round=15');
    req.flush(new Blob(['date\n']), {
      headers: {
        'content-disposition': 'attachment; filename="clockopus-2026-11-01-2026-11-30.csv"',
      },
    });
    await done;
    expect(click).toHaveBeenCalledTimes(1);
    const anchor = click.mock.instances[0] as unknown as HTMLAnchorElement;
    expect(anchor.download).toBe('clockopus-2026-11-01-2026-11-30.csv');
    click.mockRestore();
  });

  it('imports YAML as YAML and deletes the account', async () => {
    const imported = service.importData('version: 1\n');
    const req = http.expectOne('/api/v1/import');
    expect(req.request.method).toBe('POST');
    expect(req.request.headers.get('content-type')).toBe('application/yaml');
    expect(req.request.body).toBe('version: 1\n');
    req.flush(null, { status: 204, statusText: 'No Content' });
    await imported;

    const deleted = service.deleteAccount();
    const del = http.expectOne('/api/v1/me');
    expect(del.request.method).toBe('DELETE');
    del.flush(null, { status: 204, statusText: 'No Content' });
    await deleted;
  });
});
