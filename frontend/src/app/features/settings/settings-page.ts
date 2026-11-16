import { Component, computed, inject, linkedSignal, signal } from '@angular/core';
import { AccountService } from '../../core/account.service';
import { monthRange, localDate } from '../../core/local-time';
import { SessionService } from '../../core/session.service';
import { TasksService, errorMessage } from '../../core/tasks.service';
import { WeekStart } from '../../core/wire';

type Notice = { kind: 'ok' | 'error'; text: string } | null;

/** Settings, exports, import and account deletion (docs/SPEC.md, section 4.4). */
@Component({
  selector: 'app-settings-page',
  templateUrl: './settings-page.html',
  styleUrl: './settings-page.css',
})
export class SettingsPage {
  protected readonly session = inject(SessionService);
  private readonly account = inject(AccountService);
  private readonly tasks = inject(TasksService);

  readonly zones: string[] = supportedZones();

  readonly timezone = linkedSignal(() => this.session.settings()?.timezone ?? '');
  readonly workdayEnd = linkedSignal(() => this.session.settings()?.workday_end ?? '17:00');
  readonly forgottenAfter = linkedSignal(
    () => this.session.settings()?.forgotten_after_hours ?? 10,
  );
  readonly weekStart = linkedSignal<WeekStart>(
    () => this.session.settings()?.week_start ?? 'monday',
  );
  readonly csvRounding = linkedSignal(() => this.session.settings()?.csv_rounding ?? 0);

  readonly csvFrom = signal(monthRange(localDate(new Date()))[0]);
  readonly csvTo = signal(monthRange(localDate(new Date()))[1]);
  readonly csvRound = linkedSignal(() => this.session.settings()?.csv_rounding ?? 0);

  readonly settingsNotice = signal<Notice>(null);
  readonly dataNotice = signal<Notice>(null);
  readonly busy = signal(false);

  readonly browserZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  readonly zoneDiffers = computed(() => !!this.timezone() && this.timezone() !== this.browserZone);

  async saveSettings(): Promise<void> {
    try {
      await this.session.saveSettings({
        timezone: this.timezone().trim(),
        workday_end: this.workdayEnd(),
        forgotten_after_hours: Number(this.forgottenAfter()),
        week_start: this.weekStart(),
        csv_rounding: Number(this.csvRounding()),
      });
      this.settingsNotice.set({ kind: 'ok', text: 'Settings saved.' });
      // "Today" and the forgotten timer depend on these settings.
      void this.tasks.load();
    } catch (e) {
      this.settingsNotice.set({ kind: 'error', text: errorMessage(e) });
    }
  }

  async downloadCsv(): Promise<void> {
    await this.run(() =>
      this.account.downloadCsv(this.csvFrom(), this.csvTo(), Number(this.csvRound())),
    );
  }

  async downloadExport(): Promise<void> {
    await this.run(() => this.account.downloadExport());
  }

  async importFile(input: HTMLInputElement): Promise<void> {
    const file = input.files?.[0];
    if (!file) return;
    const text = await file.text();
    input.value = '';
    const ok = await this.run(() => this.account.importData(text));
    if (ok) {
      this.dataNotice.set({ kind: 'ok', text: `Imported ${file.name}.` });
      await this.session.load();
      await this.tasks.load();
    }
  }

  async deleteAccount(): Promise<void> {
    const sure = confirm(
      'Delete your account with all tasks, times, tags and settings? This cannot be undone. ' +
        'Export your data first if you want to keep it.',
    );
    if (!sure) return;
    const ok = await this.run(() => this.account.deleteAccount());
    if (ok) {
      this.dataNotice.set({ kind: 'ok', text: 'Your data has been deleted.' });
      await this.session.load();
      await this.tasks.load();
    }
  }

  private async run(op: () => Promise<unknown>): Promise<boolean> {
    this.busy.set(true);
    this.dataNotice.set(null);
    try {
      await op();
      return true;
    } catch (e) {
      this.dataNotice.set({ kind: 'error', text: errorMessage(e) });
      return false;
    } finally {
      this.busy.set(false);
    }
  }
}

function supportedZones(): string[] {
  const intl = Intl as unknown as { supportedValuesOf?: (key: string) => string[] };
  return intl.supportedValuesOf ? intl.supportedValuesOf('timeZone') : [];
}
