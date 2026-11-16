import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { HttpErrorResponse } from '@angular/common/http';
import { AccountService } from '../../core/account.service';
import { SessionService } from '../../core/session.service';
import { TasksService } from '../../core/tasks.service';
import { Settings } from '../../core/wire';
import { SettingsPage } from './settings-page';

describe('SettingsPage', () => {
  let fixture: ComponentFixture<SettingsPage>;
  const settings = signal<Settings | null>({
    timezone: 'Europe/Berlin',
    workday_end: '17:00',
    forgotten_after_hours: 10,
    week_start: 'monday',
    csv_rounding: 0,
  });
  const session = {
    settings,
    saveSettings: vi.fn().mockResolvedValue(undefined),
    load: vi.fn().mockResolvedValue(undefined),
  };
  const account = {
    downloadCsv: vi.fn().mockResolvedValue(undefined),
    downloadExport: vi.fn().mockResolvedValue(undefined),
    importData: vi.fn().mockResolvedValue(undefined),
    deleteAccount: vi.fn().mockResolvedValue(undefined),
  };
  const tasks = { load: vi.fn().mockResolvedValue(undefined) };

  beforeEach(async () => {
    vi.clearAllMocks();
    TestBed.configureTestingModule({
      providers: [
        { provide: SessionService, useValue: session },
        { provide: AccountService, useValue: account },
        { provide: TasksService, useValue: tasks },
      ],
    });
    fixture = TestBed.createComponent(SettingsPage);
    await fixture.whenStable();
  });

  const el = () => fixture.nativeElement as HTMLElement;

  it('should create with the current settings', () => {
    expect(fixture.componentInstance).toBeTruthy();
    const tz = el().querySelector<HTMLInputElement>('input[list=zones]')!;
    expect(tz.value).toBe('Europe/Berlin');
    expect(el().querySelector<HTMLInputElement>('input[type=time]')!.value).toBe('17:00');
  });

  it('saves all settings and reloads the board', async () => {
    const page = fixture.componentInstance;
    page.workdayEnd.set('16:30');
    page.weekStart.set('sunday');
    page.csvRounding.set(15);
    await page.saveSettings();
    expect(session.saveSettings).toHaveBeenCalledWith({
      timezone: 'Europe/Berlin',
      workday_end: '16:30',
      forgotten_after_hours: 10,
      week_start: 'sunday',
      csv_rounding: 15,
    });
    expect(tasks.load).toHaveBeenCalled();
    expect(page.settingsNotice()).toEqual({ kind: 'ok', text: 'Settings saved.' });
  });

  it('shows the server message when settings are rejected', async () => {
    session.saveSettings.mockRejectedValueOnce(
      new HttpErrorResponse({
        status: 400,
        error: { error: 'timezone_unknown', message: 'Unknown time zone `Mars/Olympus`.' },
      }),
    );
    await fixture.componentInstance.saveSettings();
    expect(fixture.componentInstance.settingsNotice()).toEqual({
      kind: 'error',
      text: 'Unknown time zone `Mars/Olympus`.',
    });
  });

  it('downloads the CSV for the chosen range and rounding', async () => {
    const page = fixture.componentInstance;
    page.csvFrom.set('2026-11-01');
    page.csvTo.set('2026-11-15');
    page.csvRound.set(5);
    await page.downloadCsv();
    expect(account.downloadCsv).toHaveBeenCalledWith('2026-11-01', '2026-11-15', 5);
  });

  it('imports a file and reloads session and board', async () => {
    const input = document.createElement('input');
    const file = new File(['version: 1\n'], 'backup.yaml');
    Object.defineProperty(input, 'files', { value: [file] });
    await fixture.componentInstance.importFile(input);
    expect(account.importData).toHaveBeenCalledWith('version: 1\n');
    expect(session.load).toHaveBeenCalled();
    expect(tasks.load).toHaveBeenCalled();
    expect(fixture.componentInstance.dataNotice()).toEqual({
      kind: 'ok',
      text: 'Imported backup.yaml.',
    });
  });

  it('shows why an import was refused', async () => {
    account.importData.mockRejectedValueOnce(
      new HttpErrorResponse({
        status: 400,
        error: {
          error: 'account_not_empty',
          message: 'Import works only into an empty account. Delete the current data first.',
        },
      }),
    );
    const input = document.createElement('input');
    Object.defineProperty(input, 'files', { value: [new File(['x'], 'x.yaml')] });
    await fixture.componentInstance.importFile(input);
    expect(fixture.componentInstance.dataNotice()?.text).toContain('only into an empty account');
    expect(tasks.load).not.toHaveBeenCalled();
  });

  it('deletes the account only after confirmation', async () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValueOnce(false);
    await fixture.componentInstance.deleteAccount();
    expect(account.deleteAccount).not.toHaveBeenCalled();
    confirmSpy.mockReturnValueOnce(true);
    await fixture.componentInstance.deleteAccount();
    expect(account.deleteAccount).toHaveBeenCalledTimes(1);
    expect(session.load).toHaveBeenCalled();
    confirmSpy.mockRestore();
  });
});
