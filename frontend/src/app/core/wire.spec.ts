import { toCreateTaskWire, toUpdateTaskWire } from './wire';

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
});
