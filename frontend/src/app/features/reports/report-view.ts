import { Component, computed, effect, inject, input, signal, untracked } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { AccountService } from '../../core/account.service';
import { formatHm, formatHours } from '../../core/format';
import {
  addDays,
  localDate,
  monthGrid,
  monthName,
  monthRange,
  shortDay,
  startOfDay,
  weekRange,
} from '../../core/local-time';
import { SessionService } from '../../core/session.service';
import { TasksService, errorMessage } from '../../core/tasks.service';
import { Report } from '../../core/wire';

export type ReportMode = 'week' | 'month' | 'range';

const isDate = (d: string | undefined): d is string => !!d && /^\d{4}-\d{2}-\d{2}$/.test(d);

/** Week, month and custom range reports (docs/SPEC.md, section 4.3). */
@Component({
  selector: 'app-report-view',
  imports: [RouterLink],
  templateUrl: './report-view.html',
  styleUrl: './report-view.css',
})
export class ReportView {
  private readonly account = inject(AccountService);
  private readonly session = inject(SessionService);
  private readonly tasks = inject(TasksService);
  private readonly router = inject(Router);

  /** Route data. */
  readonly mode = input<ReportMode>('week');
  /** Route parameter (week, month): any day inside. */
  readonly date = input<string | undefined>(undefined);
  /** Query parameters (range). */
  readonly from = input<string | undefined>(undefined);
  readonly to = input<string | undefined>(undefined);

  readonly report = signal<Report | null>(null);
  readonly error = signal<string | null>(null);

  readonly today = localDate(new Date());
  readonly weekStart = computed(() => this.session.settings()?.week_start ?? 'monday');
  readonly anchor = computed(() => {
    const d = this.date();
    return isDate(d) ? d : this.today;
  });

  readonly range = computed<[string, string]>(() => {
    switch (this.mode()) {
      case 'week':
        return weekRange(this.anchor(), this.weekStart());
      case 'month':
        return monthRange(this.anchor());
      case 'range': {
        const [first, last] = monthRange(this.today);
        const from = isDate(this.from()) ? this.from()! : first;
        const to = isDate(this.to()) ? this.to()! : last;
        return from <= to ? [from, to] : [to, from];
      }
    }
  });

  readonly heading = computed(() => {
    const [from, to] = this.range();
    if (this.mode() === 'month') return monthName(from);
    const f = startOfDay(from);
    const t = startOfDay(to);
    const sameMonth = f.getMonth() === t.getMonth() && f.getFullYear() === t.getFullYear();
    const left = f.toLocaleDateString(
      'en-GB',
      sameMonth ? { day: 'numeric' } : { day: 'numeric', month: 'long' },
    );
    return `${left} – ${t.toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric' })}`;
  });

  readonly previous = computed(() =>
    this.mode() === 'week' ? addDays(this.range()[0], -7) : addDays(this.range()[0], -1),
  );
  readonly next = computed(() => addDays(this.range()[1], 1));
  readonly isCurrent = computed(() => {
    const [from, to] = this.range();
    return this.today >= from && this.today <= to;
  });

  /** Week table: one row per task, one column per day. */
  readonly weekRows = computed(() => {
    const r = this.report();
    if (!r) return [];
    return r.by_task.map((t) => ({
      title: t.title,
      total: t.seconds,
      days: r.days.map((d) => d.by_task.find((x) => x.task_id === t.task_id)?.seconds ?? 0),
    }));
  });

  /** Month calendar: weeks of days with totals and a 0..1 intensity. */
  readonly calendar = computed(() => {
    const r = this.report();
    const totals = new Map((r?.days ?? []).map((d) => [d.date, d.total_seconds]));
    const max = Math.max(1, ...totals.values());
    const [first, last] = this.range();
    return monthGrid(this.range()[0], this.weekStart()).map((week) =>
      week.map((date) => {
        const seconds = totals.get(date) ?? 0;
        return {
          date,
          day: startOfDay(date).getDate(),
          inMonth: date >= first && date <= last,
          seconds,
          intensity: seconds / max,
        };
      }),
    );
  });

  readonly weekdayNames = computed(() =>
    monthGrid(this.range()[0], this.weekStart())[0].map((d) =>
      startOfDay(d).toLocaleDateString('en-GB', { weekday: 'short' }),
    ),
  );

  protected readonly hm = formatHm;
  protected readonly hours = formatHours;
  protected readonly shortDay = shortDay;

  constructor() {
    effect(() => {
      const [from, to] = this.range();
      this.tasks.revision();
      if (this.session.me()) untracked(() => void this.load(from, to));
    });
  }

  private async load(from: string, to: string): Promise<void> {
    try {
      this.report.set(await this.account.report(from, to));
      this.error.set(null);
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  link(date: string): string[] {
    return [`/${this.mode()}`, date];
  }

  setRange(from: string, to: string): void {
    void this.router.navigate(['/range'], { queryParams: { from, to } });
  }
}
