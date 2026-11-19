import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { taskView } from '../../../testing/fixtures';
import { localDate } from '../../core/local-time';
import { TasksService } from '../../core/tasks.service';
import { NoteView, TagView } from '../../core/wire';
import { monthsBefore, NotesPage, periodRange } from './notes-page';

function note(over: Partial<NoteView> = {}): NoteView {
  return {
    ...taskView({
      id: 'n1',
      title: 'Planning',
      tags: ['team-a'],
      notes: 'Agenda\n  - budget',
      notes_changed_at: '2026-11-10T08:00:00Z',
    }),
    period_seconds: 5400,
    ...over,
  };
}

function tag(name: string): TagView {
  return { name, color: null, task_count: 1, recent_seconds: 0 };
}

describe('periods', () => {
  it('go back whole months, clamped to the end of a shorter month', () => {
    expect(monthsBefore('2026-11-19', 1)).toBe('2026-10-19');
    expect(monthsBefore('2026-03-31', 1)).toBe('2026-02-28');
    expect(monthsBefore('2028-03-31', 1)).toBe('2028-02-29');
    expect(monthsBefore('2026-01-15', 3)).toBe('2025-10-15');
    expect(monthsBefore('2026-11-19', 12)).toBe('2025-11-19');
  });

  it('end today, all time has none, a broken custom range falls back to a month', () => {
    const today = '2026-11-19';
    expect(periodRange('1m', today)).toEqual(['2026-10-19', today]);
    expect(periodRange('3m', today)).toEqual(['2026-08-19', today]);
    expect(periodRange('12m', today)).toEqual(['2025-11-19', today]);
    expect(periodRange('all', today)).toBeNull();
    expect(periodRange('custom', today, '2024-01-01', '2024-12-31')).toEqual([
      '2024-01-01',
      '2024-12-31',
    ]);
    expect(periodRange('custom', today, '2024-12-31', '2024-01-01')).toEqual(['2026-10-19', today]);
    expect(periodRange('custom', today, 'x', '')).toEqual(['2026-10-19', today]);
  });
});

describe('NotesPage', () => {
  let fixture: ComponentFixture<NotesPage>;
  const service = {
    revision: signal(0),
    loaded: signal(true),
    tags: signal<TagView[]>([tag('team-a'), tag('review')]),
    error: signal<string | null>(null),
    load: vi.fn(),
    clearError: vi.fn(),
    entries: vi.fn().mockResolvedValue([]),
    notes: vi.fn(),
  };
  const today = localDate(new Date());

  async function render(q: { tag?: string; period?: string; from?: string; to?: string } = {}) {
    TestBed.configureTestingModule({
      providers: [provideRouter([]), { provide: TasksService, useValue: service }],
    });
    fixture = TestBed.createComponent(NotesPage);
    for (const [k, v] of Object.entries(q)) fixture.componentRef.setInput(k, v);
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
    return fixture.nativeElement as HTMLElement;
  }

  beforeEach(() => {
    vi.clearAllMocks();
    service.notes.mockResolvedValue([note()]);
  });

  it('should create and load the last month for all tags by default', async () => {
    const el = await render();
    expect(fixture.componentInstance).toBeTruthy();
    expect(service.notes).toHaveBeenCalledWith(null, [monthsBefore(today, 1), today]);
    expect(el.querySelector('.summary')?.textContent).toContain('1 task with notes');
    expect(el.querySelector('.summary')?.textContent).toContain('with any tag');
  });

  it('shows the notes text with its line breaks, the tags and the time', async () => {
    const el = await render({ tag: 'team-a' });
    const article = el.querySelector('article.note')!;
    expect(article.querySelector('h2')?.textContent).toBe('Planning');
    // pre-wrap keeps the lines; the text itself is unchanged.
    expect(article.querySelector('.text')?.textContent).toBe('Agenda\n  - budget');
    expect(getComputedStyle(article.querySelector('.text')!).whiteSpace).toBe('pre-wrap');
    expect(article.querySelector('.meta')?.textContent).toContain('#team-a');
    expect(article.querySelector('.meta')?.textContent).toContain('notes changed 10 Nov 2026');
    expect(article.querySelector('.meta')?.textContent).toContain('1h30m in this period');
  });

  it('takes tag and period from the URL', async () => {
    await render({ tag: '#Team-A', period: '12m' });
    expect(service.notes).toHaveBeenCalledWith('team-a', [monthsBefore(today, 12), today]);
    const selects = fixture.nativeElement.querySelectorAll(
      'select',
    ) as NodeListOf<HTMLSelectElement>;
    expect(selects[0].value).toBe('team-a');
    expect(selects[1].value).toBe('12m');
  });

  it('asks for all time without a period', async () => {
    const el = await render({ tag: 'review', period: 'all' });
    expect(service.notes).toHaveBeenCalledWith('review', null);
    expect(el.querySelector('.summary')?.textContent).toContain('all time');
    expect(el.querySelector('.meta')?.textContent).toContain('in total');
  });

  it('puts a new choice into the URL', async () => {
    const el = await render({ tag: 'team-a' });
    const router = TestBed.inject(Router);
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    const [tagSelect, periodSelect] = el.querySelectorAll<HTMLSelectElement>('select');

    periodSelect.value = '3m';
    periodSelect.dispatchEvent(new Event('change'));
    expect(navigate).toHaveBeenLastCalledWith(['/notes'], {
      queryParams: { tag: 'team-a', period: '3m' },
    });

    tagSelect.value = '';
    tagSelect.dispatchEvent(new Event('change'));
    expect(navigate).toHaveBeenLastCalledWith(['/notes'], { queryParams: {} });
  });

  it('offers dates for a custom range', async () => {
    const el = await render({ period: 'custom', from: '2024-01-01', to: '2024-12-31' });
    expect(service.notes).toHaveBeenCalledWith(null, ['2024-01-01', '2024-12-31']);
    const dates = el.querySelectorAll<HTMLInputElement>('input[type=date]');
    expect([...dates].map((d) => d.value)).toEqual(['2024-01-01', '2024-12-31']);
  });

  it('opens the edit dialog for a note and reloads after a change', async () => {
    const el = await render();
    const edit = [...el.querySelectorAll('article button')].find(
      (b) => b.textContent?.trim() === 'Edit',
    ) as HTMLButtonElement;
    edit.click();
    expect(fixture.componentInstance.editing()?.id).toBe('n1');

    expect(service.notes).toHaveBeenCalledTimes(1);
    service.revision.update((r) => r + 1);
    await fixture.whenStable();
    expect(service.notes).toHaveBeenCalledTimes(2);
  });

  it('explains where notes come from when there are none', async () => {
    service.notes.mockResolvedValue([]);
    const el = await render({ tag: 'review' });
    expect(el.querySelectorAll('article.note').length).toBe(0);
    expect(el.querySelector('.empty')?.textContent).toContain('Edit');
    expect(el.querySelector('.summary')?.textContent).toContain('0 tasks with notes');
  });
});
