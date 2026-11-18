import { TestBed } from '@angular/core/testing';
import { SelectionService } from './selection.service';

describe('SelectionService', () => {
  let s: SelectionService;

  beforeEach(() => {
    s = TestBed.inject(SelectionService);
    s.clear();
  });

  it('highlights everything without a selection', () => {
    expect(s.active()).toBe(false);
    expect(s.matches('t1', [])).toBe(true);
  });

  it('a click picks one, a second click on it clears', () => {
    s.toggleTask('t1', false);
    expect([...s.tasks()]).toEqual(['t1']);
    s.toggleTask('t2', false);
    expect([...s.tasks()]).toEqual(['t2']);
    s.toggleTag('review', false);
    expect([...s.tasks()]).toEqual([]);
    expect([...s.tags()]).toEqual(['review']);
    s.toggleTag('review', false);
    expect(s.active()).toBe(false);
  });

  it('Ctrl/⌘+click adds and removes, across tasks and tags', () => {
    s.toggleTask('t1', false);
    s.toggleTask('t2', true);
    s.toggleTag('review', true);
    expect([...s.tasks()]).toEqual(['t1', 't2']);
    expect([...s.tags()]).toEqual(['review']);
    s.toggleTask('t1', true);
    expect([...s.tasks()]).toEqual(['t2']);
  });

  it('matches a picked task, any picked tag, or untagged', () => {
    s.toggleTask('t1', false);
    s.toggleTag('review', true);
    expect(s.matches('t1', [])).toBe(true);
    expect(s.matches('t9', ['team-a', 'review'])).toBe(true);
    expect(s.matches('t9', ['team-a'])).toBe(false);
    expect(s.matches('t9', [])).toBe(false);
    s.toggleTag(null, true);
    expect(s.matches('t9', [])).toBe(true);
  });
});
