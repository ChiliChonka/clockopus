import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { AccountService } from '../../core/account.service';
import { SessionService } from '../../core/session.service';
import { TasksService } from '../../core/tasks.service';
import { Report, Settings } from '../../core/wire';
import { ReportView } from './report-view';

function report(from: string, days: string[]): Report {
  return {
    from,
    to: days[days.length - 1],
    total_seconds: 5400 + 1800,
    days: days.map((date, i) => ({
      date,
      total_seconds: i === 0 ? 5400 : i === 2 ? 1800 : 0,
      by_task:
        i === 0
          ? [{ task_id: 't1', seconds: 5400 }]
          : i === 2
            ? [{ task_id: 't2', seconds: 1800 }]
            : [],
    })),
    by_task: [
      { task_id: 't1', title: 'Review', tags: ['review'], seconds: 5400 },
      { task_id: 't2', title: 'Mail', tags: [], seconds: 1800 },
    ],
    by_tag: [
      { tag: 'review', seconds: 5400 },
      { tag: null, seconds: 1800 },
    ],
  };
}

describe('ReportView', () => {
  let fixture: ComponentFixture<ReportView>;
  const settings = signal<Partial<Settings> | null>({ week_start: 'monday' });
  const account = { report: vi.fn() };
  const week = [
    '2026-11-16',
    '2026-11-17',
    '2026-11-18',
    '2026-11-19',
    '2026-11-20',
    '2026-11-21',
    '2026-11-22',
  ];

  async function render(
    mode: 'week' | 'month' | 'range',
    date?: string,
    q: { from?: string; to?: string } = {},
  ) {
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        { provide: AccountService, useValue: account },
        { provide: TasksService, useValue: { revision: signal(0) } },
        {
          provide: SessionService,
          useValue: { me: signal({ id: 'local', name: 'Local user' }), settings },
        },
      ],
    });
    fixture = TestBed.createComponent(ReportView);
    fixture.componentRef.setInput('mode', mode);
    if (date) fixture.componentRef.setInput('date', date);
    if (q.from) fixture.componentRef.setInput('from', q.from);
    if (q.to) fixture.componentRef.setInput('to', q.to);
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
    return fixture.nativeElement as HTMLElement;
  }

  beforeEach(() => {
    vi.clearAllMocks();
    settings.set({ week_start: 'monday' });
    account.report.mockImplementation((from: string) => Promise.resolve(report(from, week)));
  });

  it('should create a week report for the week of the date', async () => {
    const el = await render('week', '2026-11-18');
    expect(fixture.componentInstance).toBeTruthy();
    expect(account.report).toHaveBeenCalledWith('2026-11-16', '2026-11-22');
    expect(el.querySelector('h2')!.textContent).toBe('16 – 22 November 2026');
  });

  it('follows the week start setting', async () => {
    settings.set({ week_start: 'sunday' });
    await render('week', '2026-11-18');
    expect(account.report).toHaveBeenCalledWith('2026-11-15', '2026-11-21');
  });

  it('shows tasks by day with totals', async () => {
    const el = await render('week', '2026-11-18');
    const rows = [...el.querySelectorAll('tbody tr')].map((tr) =>
      [...tr.children].map((c) => c.textContent!.trim()),
    );
    expect(rows).toEqual([
      ['Review', '1h30m', '–', '–', '–', '–', '–', '–', '1h30m'],
      ['Mail', '–', '–', '30m', '–', '–', '–', '–', '30m'],
    ]);
    const foot = [...el.querySelectorAll('tfoot td')].map((c) => c.textContent!.trim());
    expect(foot).toEqual(['1h30m', '0m', '30m', '0m', '0m', '0m', '0m', '2h']);
    const links = [...el.querySelectorAll('thead a')].map((a) => a.getAttribute('href'));
    expect(links[0]).toBe('/day/2026-11-16');
  });

  it('lists hours per tag including untagged', async () => {
    const el = await render('week', '2026-11-18');
    const tags = [...el.querySelectorAll('aside li')].map((li) =>
      [...li.querySelectorAll('span')].map((s) => s.textContent!.trim()).join(' '),
    );
    expect(tags).toEqual(['#review 1h30m', 'untagged 30m']);
  });

  it('shows a month as a calendar with day totals linking to the day', async () => {
    const el = await render('month', '2026-11-18');
    expect(account.report).toHaveBeenCalledWith('2026-11-01', '2026-11-30');
    expect(el.querySelector('h2')!.textContent).toBe('November 2026');
    const cells = el.querySelectorAll<HTMLAnchorElement>('.calendar a.cell');
    expect(cells.length).toBe(42);
    const nov16 = [...cells].find((c) => c.getAttribute('href') === '/day/2026-11-16')!;
    expect(nov16.querySelector('.sum')!.textContent).toBe('1h30m');
    expect(nov16.style.getPropertyValue('--heat')).toBe('1');
    const oct26 = [...cells].find((c) => c.getAttribute('href') === '/day/2026-10-26')!;
    expect(oct26.classList).toContain('out');
  });

  it('navigates to the previous and next month', async () => {
    const el = await render('month', '2026-11-18');
    const prev = el.querySelector('a[aria-label="Previous month"]')!.getAttribute('href');
    const next = el.querySelector('a[aria-label="Next month"]')!.getAttribute('href');
    expect(prev).toBe('/month/2026-10-31');
    expect(next).toBe('/month/2026-12-01');
  });

  it('uses from and to for a custom range and swaps them if reversed', async () => {
    await render('range', undefined, { from: '2026-11-20', to: '2026-11-10' });
    expect(account.report).toHaveBeenCalledWith('2026-11-10', '2026-11-20');
  });

  it('shows an empty period', async () => {
    account.report.mockResolvedValue({
      ...report('2026-11-16', week),
      by_task: [],
      by_tag: [],
      total_seconds: 0,
    });
    const el = await render('week', '2026-11-18');
    expect(el.textContent).toContain('Nothing tracked in this period.');
  });

  function click(el: HTMLElement, selector: string, label: string, init: MouseEventInit = {}) {
    const button = [...el.querySelectorAll<HTMLButtonElement>(selector)].find((b) =>
      b.textContent!.trim().startsWith(label),
    )!;
    button.dispatchEvent(new MouseEvent('click', { bubbles: true, ...init }));
  }

  it('week: a click on a task highlights its row and shows the selected time per day', async () => {
    const el = await render('week', '2026-11-18');
    click(el, 'tbody th button', 'Mail');
    await fixture.whenStable();
    const dimmedRows = [...el.querySelectorAll('tbody tr')].map((tr) =>
      tr.classList.contains('dimmed'),
    );
    expect(dimmedRows).toEqual([true, false]);
    const selected = [...el.querySelectorAll('tfoot tr.selected-row td')].map((c) =>
      c.textContent!.trim(),
    );
    expect(selected).toEqual(['0m', '0m', '30m', '0m', '0m', '0m', '0m', '30m']);
  });

  it('week: tags with Ctrl+click add up', async () => {
    const el = await render('week', '2026-11-18');
    click(el, 'aside .pickable button', '#review');
    click(el, 'aside .pickable button', 'untagged', { ctrlKey: true });
    await fixture.whenStable();
    expect([...el.querySelectorAll('tbody tr.dimmed')].length).toBe(0);
    expect(el.querySelector('aside .total-line + .total-line strong')!.textContent).toBe('2h');
  });

  it('month: the calendar shows only the highlighted time', async () => {
    const el = await render('month', '2026-11-18');
    click(el, 'aside .pickable button', 'Mail');
    await fixture.whenStable();
    const cells = [...el.querySelectorAll<HTMLAnchorElement>('.calendar a.cell')];
    const nov16 = cells.find((c) => c.getAttribute('href') === '/day/2026-11-16')!;
    const nov18 = cells.find((c) => c.getAttribute('href') === '/day/2026-11-18')!;
    expect(nov16.querySelector('.sum')).toBeNull();
    expect(nov18.querySelector('.sum')!.textContent).toBe('30m');
    expect(nov18.style.getPropertyValue('--heat')).toBe('1');
  });
});
