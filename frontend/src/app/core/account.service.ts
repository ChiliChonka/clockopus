import { HttpClient, HttpResponse } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';
import { firstValueFrom } from 'rxjs';
import { Report } from './wire';

/** Reports, downloads, import and account deletion (docs/SPEC.md, sections 4.3 and 4.4). */
@Injectable({ providedIn: 'root' })
export class AccountService {
  private readonly http = inject(HttpClient);

  report(from: string, to: string): Promise<Report> {
    return firstValueFrom(this.http.get<Report>('/api/v1/reports', { params: { from, to } }));
  }

  /** Downloads the CSV through the API (with the session's credentials) and saves it. */
  async downloadCsv(from: string, to: string, round: number): Promise<void> {
    const res = await firstValueFrom(
      this.http.get('/api/v1/export.csv', {
        params: { from, to, round },
        observe: 'response',
        responseType: 'blob',
      }),
    );
    saveDownload(res, `clockopus-${from}-${to}.csv`);
  }

  async downloadExport(): Promise<void> {
    const res = await firstValueFrom(
      this.http.get('/api/v1/export', { observe: 'response', responseType: 'blob' }),
    );
    saveDownload(res, 'clockopus-export.yaml');
  }

  importData(yaml: string): Promise<unknown> {
    return firstValueFrom(
      this.http.post('/api/v1/import', yaml, { headers: { 'content-type': 'application/yaml' } }),
    );
  }

  deleteAccount(): Promise<unknown> {
    return firstValueFrom(this.http.delete('/api/v1/me'));
  }
}

/** `attachment; filename="x.csv"` → `x.csv`. */
export function filenameFrom(disposition: string | null, fallback: string): string {
  const m = disposition && /filename="([^"]+)"/.exec(disposition);
  return m ? m[1] : fallback;
}

function saveDownload(res: HttpResponse<Blob>, fallback: string): void {
  const url = URL.createObjectURL(res.body!);
  const a = document.createElement('a');
  a.href = url;
  a.download = filenameFrom(res.headers.get('content-disposition'), fallback);
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
