import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { taskView } from '../../testing/fixtures';
import { TasksService } from './tasks.service';
import { TaskView } from './wire';

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

  async function flushLoad(tasks: TaskView[] = [], status = 'active') {
    await Promise.resolve();
    http.expectOne(`/api/v1/tasks?status=${status}`).flush(tasks);
    http.expectOne('/api/v1/tags').flush([]);
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
});
