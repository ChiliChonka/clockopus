import { ComponentFixture, TestBed } from '@angular/core/testing';
import { taskView } from '../../../testing/fixtures';
import { TaskView } from '../../core/wire';
import { TaskCard } from './task-card';

describe('TaskCard', () => {
  let fixture: ComponentFixture<TaskCard>;
  const now = Date.parse('2026-11-17T10:00:00Z');

  async function render(task: TaskView, loadedAt = now) {
    fixture = TestBed.createComponent(TaskCard);
    fixture.componentRef.setInput('task', task);
    fixture.componentRef.setInput('now', now);
    fixture.componentRef.setInput('loadedAt', loadedAt);
    await fixture.whenStable();
    return fixture.nativeElement as HTMLElement;
  }

  function button(el: HTMLElement, text: string): HTMLButtonElement {
    const found = [...el.querySelectorAll('button')].find((b) => b.textContent?.trim() === text);
    if (!found) throw new Error(`no button "${text}"`);
    return found;
  }

  it('should create and show title, tags and time today', async () => {
    const el = await render(taskView({ tags: ['review', 'team-a'], today_seconds: 65 * 60 }));
    expect(fixture.componentInstance).toBeTruthy();
    expect(el.querySelector('h3')?.textContent).toBe('Fix login bug');
    expect([...el.querySelectorAll('.tags .chip')].map((c) => c.textContent)).toEqual([
      '#review',
      '#team-a',
    ]);
    expect(el.querySelector('.time')?.textContent).toContain('today 1:05');
    expect(el.querySelector('.elapsed')).toBeNull();
  });

  it('shows the running time and adds it to today live', async () => {
    const el = await render(
      taskView({ running_since: '2026-11-17T09:30:00Z', today_seconds: 30 * 60 }),
      now - 10 * 60 * 1000,
    );
    expect(el.querySelector('.elapsed')?.textContent).toBe('0:30:00');
    // 30 min at load time + 10 min since then.
    expect(el.querySelector('.time')?.textContent).toContain('today 0:40');
    expect(el.querySelector('article')?.classList).toContain('is-running');
  });

  it('emits start or stop depending on the state', async () => {
    let el = await render(taskView());
    const start = vi.fn();
    fixture.componentInstance.start.subscribe(start);
    button(el, 'Start').click();
    expect(start).toHaveBeenCalledTimes(1);

    el = await render(taskView({ running_since: '2026-11-17T09:30:00Z' }));
    const stop = vi.fn();
    fixture.componentInstance.stop.subscribe(stop);
    button(el, 'Stop').click();
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it('offers pin, done and delete in the menu', async () => {
    const el = await render(taskView({ pinned: true }));
    const pin = vi.fn();
    const done = vi.fn();
    const remove = vi.fn();
    fixture.componentInstance.pin.subscribe(pin);
    fixture.componentInstance.done.subscribe(done);
    fixture.componentInstance.remove.subscribe(remove);

    button(el, 'Unpin').click();
    button(el, 'Mark done').click();
    button(el, 'Delete').click();
    expect(pin).toHaveBeenCalledWith(false);
    expect(done).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledTimes(1);
  });

  it('opens links safely in a new tab', async () => {
    const el = await render(taskView({ link: 'https://jira.example.com/ABC-1' }));
    const a = el.querySelector('header a') as HTMLAnchorElement;
    expect(a.href).toBe('https://jira.example.com/ABC-1');
    expect(a.rel).toBe('noopener noreferrer');
    expect(a.target).toBe('_blank');
  });

  it('has no "Mark done" for a done task', async () => {
    const el = await render(taskView({ status: 'done' }));
    expect(() => button(el, 'Mark done')).toThrow();
    expect(el.querySelector('article')?.classList).toContain('is-done');
  });
});
