import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { entryView } from '../../../testing/fixtures';
import { SessionService } from '../../core/session.service';
import { TasksService } from '../../core/tasks.service';
import { DayView } from './day-view';

describe('DayView', () => {
  let fixture: ComponentFixture<DayView>;
  const local = (h: number, m = 0) => new Date(2026, 10, 10, h, m).toISOString();
  const service = {
    error: signal<string | null>(null),
    revision: signal(0),
    entriesBetween: vi.fn(),
    updateEntry: vi.fn().mockResolvedValue(true),
    deleteEntry: vi.fn().mockResolvedValue(true),
  };

  beforeEach(async () => {
    vi.clearAllMocks();
    service.entriesBetween.mockResolvedValue([
      entryView({
        id: 'e1',
        task_id: 't1',
        task_title: 'Review',
        tags: ['review', 'team-a'],
        start: local(9),
        end: local(10, 30),
      }),
      entryView({
        id: 'e2',
        task_id: 't2',
        task_title: 'Mail',
        start: local(11),
        end: local(11, 30),
      }),
    ]);
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        { provide: TasksService, useValue: service },
        { provide: SessionService, useValue: { me: signal({ id: 'local', name: 'Local user' }) } },
      ],
    });
    fixture = TestBed.createComponent(DayView);
    fixture.componentRef.setInput('date', '2026-11-10');
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
  });

  const el = () => fixture.nativeElement as HTMLElement;

  it('should create and load the day from the route', () => {
    expect(fixture.componentInstance).toBeTruthy();
    expect(service.entriesBetween).toHaveBeenCalledWith('2026-11-10', '2026-11-10');
    expect(el().querySelector('h2')!.textContent).toBe('Tuesday, 10 November 2026');
  });

  it('draws one block per entry at its time', () => {
    const blocks = el().querySelectorAll<HTMLElement>('.block');
    expect(blocks.length).toBe(2);
    // Range starts at 08:00, 1 px per minute.
    expect(blocks[0].style.top).toBe('60px');
    expect(blocks[0].style.height).toBe('90px');
    expect(blocks[0].textContent).toContain('09:00–10:30 · 1:30');
  });

  it('sums per task and per tag', () => {
    const aside = el().querySelector('aside')!;
    expect(aside.querySelector('.total')!.textContent).toContain('2:00');
    // Columns are separate spans; read them as such.
    const rows = [...aside.querySelectorAll('li')].map((li) =>
      [...li.querySelectorAll('span')]
        .map((s) => s.textContent!.trim())
        .filter(Boolean)
        .join(' '),
    );
    expect(rows).toEqual([
      'Review 1:30',
      'Mail 0:30',
      '#review 1.5 h',
      '#team-a 1.5 h',
      'untagged 0.5 h',
    ]);
  });

  it('dragging the bottom edge sets a new end, snapped to 5 minutes', async () => {
    const view = fixture.componentInstance;
    const handle = el().querySelector<HTMLElement>('.block .handle.bottom')!;
    handle.dispatchEvent(new PointerEvent('pointerdown', { clientY: 200, bubbles: true }));
    const timeline = el().querySelector<HTMLElement>('.timeline')!;
    timeline.dispatchEvent(new PointerEvent('pointermove', { clientY: 222, bubbles: true }));
    await fixture.whenStable();
    expect(view.preview()).toEqual({ id: 'e1', edge: 'end', minutes: 10 * 60 + 50 });
    expect(el().querySelector('.block')!.textContent).toContain('09:00–10:50');
    timeline.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    expect(service.updateEntry).toHaveBeenCalledWith('e1', { end: new Date(2026, 10, 10, 10, 50) });
  });

  it('cannot drag an end past the next start of the same block', async () => {
    const handle = el().querySelector<HTMLElement>('.block .handle.top')!;
    handle.dispatchEvent(new PointerEvent('pointerdown', { clientY: 100, bubbles: true }));
    const timeline = el().querySelector<HTMLElement>('.timeline')!;
    timeline.dispatchEvent(new PointerEvent('pointermove', { clientY: 400, bubbles: true }));
    expect(fixture.componentInstance.preview()!.minutes).toBe(10 * 60 + 25);
  });

  it('a click without movement saves nothing; selecting opens the editor', async () => {
    const handle = el().querySelector<HTMLElement>('.block .handle.bottom')!;
    handle.dispatchEvent(new PointerEvent('pointerdown', { clientY: 200, bubbles: true }));
    el()
      .querySelector('.timeline')!
      .dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    expect(service.updateEntry).not.toHaveBeenCalled();

    el().querySelector<HTMLButtonElement>('.block button')!.click();
    await fixture.whenStable();
    expect(el().querySelector('.editor app-entry-row')).not.toBeNull();
  });

  it('shows an empty day', async () => {
    service.entriesBetween.mockResolvedValue([]);
    fixture.componentRef.setInput('date', '2026-11-11');
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
    expect(el().textContent).toContain('Nothing tracked on this day.');
  });
});
