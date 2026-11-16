import { computed, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Title } from '@angular/platform-browser';
import { tagView, taskView } from '../../../testing/fixtures';
import { Me, SessionService } from '../../core/session.service';
import { TasksService } from '../../core/tasks.service';
import { TagView, TaskView } from '../../core/wire';
import { Board } from './board';

describe('Board', () => {
  let fixture: ComponentFixture<Board>;
  const me = signal<Me | null>(null);
  const tasks = signal<TaskView[]>([]);
  const tags = signal<TagView[]>([]);
  const loaded = signal(false);
  const showDone = signal(false);
  const error = signal<string | null>(null);
  const service = {
    tasks,
    tags,
    loaded,
    showDone,
    error,
    loadedAt: signal(Date.now()),
    running: computed(() => tasks().find((t) => t.running_since) ?? null),
    quickTags: computed(() => tags().slice(0, 8)),
    load: vi.fn().mockResolvedValue(undefined),
    create: vi.fn().mockResolvedValue(true),
    start: vi.fn().mockResolvedValue(true),
    stop: vi.fn().mockResolvedValue(true),
    update: vi.fn().mockResolvedValue(true),
    remove: vi.fn().mockResolvedValue(true),
    setShowDone: vi.fn().mockResolvedValue(undefined),
  };

  beforeEach(async () => {
    me.set({ id: 'local', name: 'Local user' });
    tasks.set([]);
    tags.set([]);
    loaded.set(true);
    showDone.set(false);
    error.set(null);
    Object.values(service).forEach((v) => {
      if (typeof v === 'function' && 'mockClear' in v) (v as ReturnType<typeof vi.fn>).mockClear();
    });
    TestBed.configureTestingModule({
      providers: [
        { provide: TasksService, useValue: service },
        { provide: SessionService, useValue: { me } },
      ],
    });
    fixture = TestBed.createComponent(Board);
    await fixture.whenStable();
  });

  afterEach(() => fixture.destroy());

  const el = () => fixture.nativeElement as HTMLElement;

  it('should create and load tasks once the session is ready', () => {
    expect(fixture.componentInstance).toBeTruthy();
    expect(service.load).toHaveBeenCalledTimes(1);
  });

  it('shows the empty state without tasks', () => {
    expect(el().querySelector('.empty h2')?.textContent).toBe('No tasks yet');
    expect(el().querySelector('app-task-card')).toBeNull();
  });

  it('shows one card per task', async () => {
    tasks.set([taskView({ id: 'a', title: 'A' }), taskView({ id: 'b', title: 'B' })]);
    await fixture.whenStable();
    expect(el().querySelectorAll('app-task-card').length).toBe(2);
    expect(el().querySelector('.empty')).toBeNull();
  });

  it('creates from the quick-add field and clears it on success', async () => {
    fixture.componentInstance.text.set('Review #review');
    await fixture.componentInstance.create({ text: 'Review #review', start: true });
    expect(service.create).toHaveBeenCalledWith('Review #review', true);
    expect(fixture.componentInstance.text()).toBe('');

    service.create.mockResolvedValueOnce(false);
    fixture.componentInstance.text.set('#only');
    await fixture.componentInstance.create({ text: '#only', start: false });
    // Kept, so the user can correct it.
    expect(fixture.componentInstance.text()).toBe('#only');
  });

  it('filters by title words and #tag prefix', async () => {
    tasks.set([
      taskView({ id: 'a', title: 'Fix login bug', tags: ['support'] }),
      taskView({ id: 'b', title: 'Review PR', tags: ['review'] }),
    ]);
    fixture.componentInstance.filter.set('#sup');
    expect(fixture.componentInstance.visible().map((t) => t.id)).toEqual(['a']);
    fixture.componentInstance.filter.set('review');
    expect(fixture.componentInstance.visible().map((t) => t.id)).toEqual(['b']);
    fixture.componentInstance.filter.set('fix bug');
    expect(fixture.componentInstance.visible().map((t) => t.id)).toEqual(['a']);
    fixture.componentInstance.filter.set('nothing');
    await fixture.whenStable();
    expect(el().textContent).toContain('No task matches');
  });

  it('shows the running timer in the tab title', async () => {
    const since = new Date(Date.now() - 42 * 60 * 1000 - 5000).toISOString();
    tasks.set([taskView({ title: 'Fix login bug', running_since: since })]);
    fixture.componentInstance.now.set(Date.now());
    await fixture.whenStable();
    expect(TestBed.inject(Title).getTitle()).toBe('▶ 0:42 Fix login bug');

    tasks.set([taskView()]);
    await fixture.whenStable();
    expect(TestBed.inject(Title).getTitle()).toBe('clockopus');
  });

  it('asks before deleting and mentions the tracked time', () => {
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValueOnce(false);
    fixture.componentInstance.remove(taskView({ id: 'a', title: 'A', total_seconds: 5400 }));
    expect(confirmSpy).toHaveBeenCalledWith('Delete “A” and its 1:30 h? This cannot be undone.');
    expect(service.remove).not.toHaveBeenCalled();

    confirmSpy.mockReturnValueOnce(true);
    fixture.componentInstance.remove(taskView({ id: 'a', title: 'A' }));
    expect(service.remove).toHaveBeenCalledWith('a');
    confirmSpy.mockRestore();
  });

  it('focuses the quick-add field with n, but not while typing elsewhere', async () => {
    tasks.set([taskView()]);
    await fixture.whenStable();
    const quick = el().querySelector<HTMLInputElement>('#quick-add')!;
    const filter = el().querySelector<HTMLInputElement>('#filter')!;
    document.body.appendChild(fixture.nativeElement);

    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', bubbles: true }));
    expect(document.activeElement).toBe(quick);

    filter.focus();
    filter.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', bubbles: true }));
    expect(document.activeElement).toBe(filter);

    filter.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }));
    expect(document.activeElement).toBe(quick);
  });

  it('shows errors from the service', async () => {
    error.set('The end cannot be in the future.');
    await fixture.whenStable();
    expect(el().querySelector('[role=alert]')?.textContent).toBe(
      'The end cannot be in the future.',
    );
  });
});
