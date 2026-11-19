import {
  toAddEntryWire,
  toCreateTaskWire,
  toStopForgottenWire,
  toUpdateEntryWire,
  toUpdateTaskWire,
} from './wire';

describe('wire format', () => {
  // The same literals are used in backend/src/api.rs (CREATE_AND_START, CREATE_ONLY).
  it('creates and starts a task like the backend expects', () => {
    expect(JSON.stringify(toCreateTaskWire('Review payment PR', ['review', 'team-a'], true))).toBe(
      '{"title":"Review payment PR","tags":["review","team-a"],"start":true}',
    );
  });

  it('creates a task without starting it', () => {
    expect(JSON.stringify(toCreateTaskWire('Write notes', [], false))).toBe(
      '{"title":"Write notes","tags":[],"start":false}',
    );
  });

  it('sends only the changed fields of a task', () => {
    expect(JSON.stringify(toUpdateTaskWire({ pinned: true }))).toBe('{"pinned":true}');
    expect(JSON.stringify(toUpdateTaskWire({ status: 'done' }))).toBe('{"status":"done"}');
    expect(JSON.stringify(toUpdateTaskWire({}))).toBe('{}');
  });

  // backend/src/api.rs: SAVE_WITH_NOTES, what the edit dialog sends on “Save task”.
  it('saves a task with notes over several lines', () => {
    const body = toUpdateTaskWire({
      title: 'Planning',
      tags: ['team-a'],
      link: '',
      notes: 'Agenda\n  - budget',
    });
    expect(JSON.stringify(body)).toBe(
      String.raw`{"title":"Planning","tags":["team-a"],"link":"","notes":"Agenda\n  - budget"}`,
    );
  });

  // backend/src/api.rs: ADD_ENTRY, MOVE_END and the forgotten-timer stop.
  it('adds a manual entry', () => {
    const body = toAddEntryWire(
      new Date('2026-11-10T08:00:00Z'),
      new Date('2026-11-10T09:30:00Z'),
      ' call with Anna ',
    );
    expect(JSON.stringify(body)).toBe(
      '{"start":"2026-11-10T08:00:00.000Z","end":"2026-11-10T09:30:00.000Z","note":"call with Anna"}',
    );
    const noNote = toAddEntryWire(
      new Date('2026-11-10T08:00:00Z'),
      new Date('2026-11-10T09:00:00Z'),
      ' ',
    );
    expect('note' in noNote).toBe(false);
  });

  it('changes only the given fields of an entry', () => {
    expect(JSON.stringify(toUpdateEntryWire({ end: new Date('2026-11-10T10:00:00Z') }))).toBe(
      '{"end":"2026-11-10T10:00:00.000Z"}',
    );
    expect(JSON.stringify(toUpdateEntryWire({ note: '' }))).toBe('{"note":""}');
  });

  it('stops a forgotten timer with the reason', () => {
    expect(JSON.stringify(toStopForgottenWire(new Date('2026-11-10T16:00:00Z')))).toBe(
      '{"end":"2026-11-10T16:00:00.000Z","reason":"forgotten-timer"}',
    );
  });
});
