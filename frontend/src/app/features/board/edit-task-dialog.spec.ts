import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { entryView, taskView } from '../../../testing/fixtures';
import { TasksService } from '../../core/tasks.service';
import { EditTaskDialog, parseTagInput } from './edit-task-dialog';

describe('EditTaskDialog', () => {
  let fixture: ComponentFixture<EditTaskDialog>;
  const service = {
    error: signal<string | null>(null),
    revision: signal(0),
    clearError: vi.fn(),
    entries: vi.fn(),
    update: vi.fn().mockResolvedValue(true),
    addEntry: vi.fn().mockResolvedValue(true),
    updateEntry: vi.fn().mockResolvedValue(true),
    deleteEntry: vi.fn().mockResolvedValue(true),
  };

  beforeEach(async () => {
    vi.clearAllMocks();
    service.entries.mockResolvedValue([entryView({ id: 'e1' }), entryView({ id: 'e2' })]);
    TestBed.configureTestingModule({ providers: [{ provide: TasksService, useValue: service }] });
    fixture = TestBed.createComponent(EditTaskDialog);
    await fixture.whenStable();
  });

  const el = () => fixture.nativeElement as HTMLElement;

  async function open() {
    fixture.componentRef.setInput(
      'task',
      taskView({ id: 't', title: 'Fix login bug', tags: ['support'], link: null }),
    );
    await fixture.whenStable();
    await new Promise((r) => setTimeout(r));
    await fixture.whenStable();
  }

  it('parses tag input with or without #', () => {
    expect(parseTagInput('#review team-a, x  ')).toEqual(['review', 'team-a', 'x']);
    expect(parseTagInput('  ')).toEqual([]);
  });

  it('should create closed and open as a modal with the task and its entries', async () => {
    expect(fixture.componentInstance).toBeTruthy();
    expect(el().querySelector('dialog')!.open).toBe(false);
    await open();
    expect(el().querySelector('dialog')!.open).toBe(true);
    expect(service.entries).toHaveBeenCalledWith('t');
    expect(el().querySelectorAll('app-entry-row').length).toBe(2);
    const inputs = el().querySelectorAll<HTMLInputElement>('form.task input');
    expect([...inputs].map((i) => i.value)).toEqual(['Fix login bug', 'support', '']);
  });

  it('saves title, tags and link', async () => {
    await open();
    const [title, tags, link] = el().querySelectorAll<HTMLInputElement>('form.task input');
    title.value = 'Fix login bug fast';
    title.dispatchEvent(new Event('input'));
    tags.value = '#support urgent';
    tags.dispatchEvent(new Event('input'));
    link.value = ' https://jira.example.com/ABC-1 ';
    link.dispatchEvent(new Event('input'));
    el()
      .querySelector('form.task')!
      .dispatchEvent(new Event('submit', { cancelable: true }));
    expect(service.update).toHaveBeenCalledWith('t', {
      title: 'Fix login bug fast',
      tags: ['support', 'urgent'],
      link: 'https://jira.example.com/ABC-1',
      notes: '',
    });
  });

  it('shows the notes and saves them with the task, also with Ctrl+Enter', async () => {
    fixture.componentRef.setInput(
      'task',
      taskView({ id: 't', title: 'Plan', notes: 'Agenda\n  - budget' }),
    );
    await fixture.whenStable();
    const area = el().querySelector<HTMLTextAreaElement>('form.task textarea')!;
    expect(area.value).toBe('Agenda\n  - budget');

    area.value = 'Agenda\n  - budget\n  - hiring';
    area.dispatchEvent(new Event('input'));
    area.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true }));
    expect(service.update).toHaveBeenCalledWith(
      't',
      expect.objectContaining({ notes: 'Agenda\n  - budget\n  - hiring' }),
    );
    // A plain Enter is a new line, not a save.
    service.update.mockClear();
    area.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(service.update).not.toHaveBeenCalled();
  });

  it('adds time from a date and two 24-hour times', async () => {
    await open();
    const set = (sel: string, v: string) => {
      const i = el().querySelector<HTMLInputElement>(sel)!;
      i.value = v;
      i.dispatchEvent(new Event('input'));
    };
    set('form.add input[type=date]', '2026-11-16');
    expect(el().querySelectorAll('form.add input[type=time]').length).toBe(0);
    const times = el().querySelectorAll<HTMLInputElement>('form.add app-time-field input');
    expect(times.length).toBe(2);
    times[0].value = '1400';
    times[0].dispatchEvent(new Event('input'));
    times[1].value = '15:30';
    times[1].dispatchEvent(new Event('input'));
    await fixture.componentInstance.addTime(taskView({ id: 't' }));
    expect(service.addEntry).toHaveBeenCalledWith(
      't',
      new Date(2026, 10, 16, 14, 0),
      new Date(2026, 10, 16, 15, 30),
      '',
    );
  });

  it('asks for both times', async () => {
    await open();
    await fixture.componentInstance.addTime(taskView({ id: 't' }));
    expect(service.addEntry).not.toHaveBeenCalled();
    expect(service.error()).toBe('Enter a date and both times (HH:MM, 24 hours).');
    service.error.set(null);
  });

  it('reloads the entries after a change', async () => {
    await open();
    expect(service.entries).toHaveBeenCalledTimes(1);
    service.revision.update((r) => r + 1);
    await fixture.whenStable();
    expect(service.entries).toHaveBeenCalledTimes(2);
  });
});
