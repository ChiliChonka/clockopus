import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { taskView } from '../../testing/fixtures';
import { TasksService } from './tasks.service';
import { TaskView, TimerView } from './wire';

describe('TasksService', () => {
  let service: TasksService;
  let http: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting()],
    });
    service = TestBed.inject(TasksService);
    http = TestBed.inject(HttpTestingController);
  });

  afterEach(() => http.verify());

  async function flushLoad(
    tasks: TaskView[] = [],
    status = 'active',
    timer: TimerView = { running: null, forgotten: null },
  ) {
    await Promise.resolve();
    http.expectOne(`/api/v1/tasks?status=${status}`).flush(tasks);
    http.expectOne('/api/v1/tags').flush([]);
    http.expectOne('/api/v1/timer').flush(timer);
  }

  it('loads tasks and tags and knows the running task', async () => {
    const done = service.load();
    await flushLoad([
      taskView({ id: 'a', running_since: '2026-11-17T09:00:00Z' }),
      taskView({ id: 'b' }),
    ]);
    await done;
    expect(service.tasks().length).toBe(2);
    expect(service.running()?.id).toBe('a');
    expect(service.loaded()).toBe(true);
  });

  it('creates a task from quick-add text and reloads', async () => {
    const done = service.create('Review payment PR #review #team-a', true);
    const req = http.expectOne('/api/v1/tasks');
    expect(req.request.method).toBe('POST');
    // Same literal as backend/src/api.rs CREATE_AND_START.
    expect(JSON.stringify(req.request.body)).toBe(
      '{"title":"Review payment PR","tags":["review","team-a"],"start":true}',
    );
    req.flush(taskView(), { status: 201, statusText: 'Created' });
    await flushLoad();
    expect(await done).toBe(true);
  });

  it('does not send a task without a title', async () => {
    expect(await service.create('#only-tags', true)).toBe(false);
    expect(service.error()).toContain('title');
  });

  it('shows the server message when a request is rejected', async () => {
    const done = service.start('x');
    http
      .expectOne('/api/v1/tasks/x/start')
      .flush({ error: 'not_found' }, { status: 404, statusText: 'Not Found' });
    await flushLoad();
    expect(await done).toBe(false);
    expect(service.error()).toBe('Something went wrong. Please try again.');

    const again = service.stop();
    http
      .expectOne('/api/v1/timer/stop')
      .flush(
        { error: 'end_in_future', message: 'The end cannot be in the future.' },
        { status: 400, statusText: 'Bad Request' },
      );
    await flushLoad();
    await again;
    expect(service.error()).toBe('The end cannot be in the future.');
  });

  it('pins, marks done and deletes with the right requests', async () => {
    let done = service.update('a', { pinned: true });
    let req = http.expectOne('/api/v1/tasks/a');
    expect(req.request.method).toBe('PATCH');
    expect(JSON.stringify(req.request.body)).toBe('{"pinned":true}');
    req.flush(taskView());
    await flushLoad();
    await done;

    done = service.remove('a');
    req = http.expectOne('/api/v1/tasks/a');
    expect(req.request.method).toBe('DELETE');
    req.flush(null, { status: 204, statusText: 'No Content' });
    await flushLoad();
    expect(await done).toBe(true);
  });

  it('includes done tasks when asked', async () => {
    const done = service.setShowDone(true);
    await flushLoad([], 'all');
    await done;
    expect(service.showDone()).toBe(true);
  });

  it('knows a forgotten timer from GET /timer', async () => {
    const done = service.load();
    // Shape as in backend/src/api.rs forgotten_timer_is_reported_and_corrected_with_history.
    await flushLoad([], 'active', {
      running: { task_id: 't', entry_id: 'e', start: '2026-11-10T07:00:00Z' },
      forgotten: {
        task_id: 't',
        task_title: 'Fix login bug',
        entry_id: 'e',
        start: '2026-11-10T07:00:00Z',
        running_seconds: 90000,
        suggested_end: '2026-11-10T16:00:00Z',
      },
    });
    await done;
    expect(service.forgotten()?.suggested_end).toBe('2026-11-10T16:00:00Z');
  });

  it('corrects a forgotten timer and keeps one running', async () => {
    let done = service.stopForgotten(new Date('2026-11-10T16:00:00Z'));
    let req = http.expectOne('/api/v1/timer/stop');
    expect(JSON.stringify(req.request.body)).toBe(
      '{"end":"2026-11-10T16:00:00.000Z","reason":"forgotten-timer"}',
    );
    req.flush({ running: null, forgotten: null });
    await flushLoad();
    expect(await done).toBe(true);

    done = service.keepRunning();
    req = http.expectOne('/api/v1/timer/keep-running');
    expect(req.request.method).toBe('POST');
    req.flush({ running: null, forgotten: null });
    await flushLoad();
    expect(await done).toBe(true);
  });

  it('adds, changes and deletes entries and bumps the revision', async () => {
    const before = service.revision();
    let done = service.addEntry(
      't',
      new Date('2026-11-10T08:00:00Z'),
      new Date('2026-11-10T09:30:00Z'),
      'call with Anna',
    );
    let req = http.expectOne('/api/v1/tasks/t/entries');
    expect(JSON.stringify(req.request.body)).toBe(
      '{"start":"2026-11-10T08:00:00.000Z","end":"2026-11-10T09:30:00.000Z","note":"call with Anna"}',
    );
    req.flush({}, { status: 201, statusText: 'Created' });
    await flushLoad();
    await done;

    done = service.updateEntry('e', { end: new Date('2026-11-10T10:00:00Z') });
    req = http.expectOne('/api/v1/entries/e');
    expect(req.request.method).toBe('PATCH');
    expect(JSON.stringify(req.request.body)).toBe('{"end":"2026-11-10T10:00:00.000Z"}');
    req.flush({});
    await flushLoad();
    await done;

    done = service.deleteEntry('e');
    req = http.expectOne('/api/v1/entries/e');
    expect(req.request.method).toBe('DELETE');
    req.flush(null, { status: 204, statusText: 'No Content' });
    await flushLoad();
    await done;
    expect(service.revision()).toBe(before + 3);
  });

  it('shows the overlap message from a 409', async () => {
    const done = service.addEntry(
      't',
      new Date('2026-11-10T09:00:00Z'),
      new Date('2026-11-10T10:00:00Z'),
      '',
    );
    http.expectOne('/api/v1/tasks/t/entries').flush(
      {
        error: 'overlap',
        message: 'Overlaps with “Write notes” from 2026-11-10 09:00 to 2026-11-10 10:30.',
        entry_id: 'x',
      },
      { status: 409, statusText: 'Conflict' },
    );
    await flushLoad();
    expect(await done).toBe(false);
    expect(service.error()).toContain('Overlaps with “Write notes”');
  });

  it('reads entries for a task and a range', async () => {
    const list = service.entries('t');
    http.expectOne('/api/v1/tasks/t/entries').flush([]);
    expect(await list).toEqual([]);
    const range = service.entriesBetween('2026-11-10', '2026-11-10');
    http.expectOne('/api/v1/entries?from=2026-11-10&to=2026-11-10').flush([]);
    expect(await range).toEqual([]);
  });
});
