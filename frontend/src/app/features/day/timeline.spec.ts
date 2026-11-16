import { entryView } from '../../../testing/fixtures';
import { blocksForDay, dayMinutes, hourRange, minuteToDate, snap, totals } from './timeline';

// Built from local Dates, so the tests hold in any time zone.
const at = (h: number, m = 0, day = 17) => new Date(2026, 10, day, h, m).toISOString();

describe('timeline', () => {
  it('places entries in minutes from midnight and clips across midnight', () => {
    const blocks = blocksForDay(
      [
        entryView({ id: 'a', task_id: 't1', start: at(9), end: at(10, 30) }),
        entryView({ id: 'b', task_id: 't2', start: at(23, 0, 16), end: at(1, 0) }),
        entryView({ id: 'c', task_id: 't1', start: at(15), end: null }),
      ],
      '2026-11-17',
      new Date(2026, 10, 17, 16, 0),
    );
    expect(blocks.map((b) => [b.entry.id, b.startMin, b.endMin])).toEqual([
      ['a', 540, 630],
      ['b', 0, 60],
      ['c', 900, 960],
    ]);
    expect(blocks[1].clippedStart).toBe(true);
    expect(blocks[2].running).toBe(true);
    expect(blocks.map((b) => b.color)).toEqual([1, 2, 1]);
  });

  it('shows at least 08-18 and widens to whole hours', () => {
    expect(hourRange([], '2026-11-17')).toEqual([8, 18]);
    const blocks = blocksForDay(
      [entryView({ start: at(6, 40), end: at(19, 10) })],
      '2026-11-17',
      new Date(2026, 10, 18),
    );
    expect(hourRange(blocks, '2026-11-17')).toEqual([6, 20]);
  });

  it('knows that a day can have 23 or 25 hours', () => {
    // Holds where the browser's zone has DST on these days (e.g. Europe/Berlin); 24 h elsewhere.
    expect([1380, 1440]).toContain(dayMinutes('2026-03-29'));
    expect([1500, 1440]).toContain(dayMinutes('2026-10-25'));
    expect(dayMinutes('2026-11-17')).toBe(1440);
  });

  it('snaps to 5 minutes and converts back to instants', () => {
    expect(snap(62)).toBe(60);
    expect(snap(63)).toBe(65);
    expect(minuteToDate('2026-11-17', 9 * 60 + 15).toISOString()).toBe(at(9, 15));
  });

  it('sums per task and gives each tag the full time (variant A)', () => {
    const blocks = blocksForDay(
      [
        entryView({
          task_id: 't1',
          task_title: 'Review',
          tags: ['review', 'team-a'],
          start: at(9),
          end: at(10),
        }),
        entryView({ task_id: 't2', task_title: 'Mail', tags: [], start: at(10), end: at(10, 30) }),
        entryView({
          task_id: 't1',
          task_title: 'Review',
          tags: ['review', 'team-a'],
          start: at(11),
          end: at(11, 30),
        }),
      ],
      '2026-11-17',
      new Date(2026, 10, 18),
    );
    const t = totals(blocks);
    expect(t.totalSeconds).toBe(2 * 3600);
    expect(t.byTask.map((x) => [x.title, x.seconds])).toEqual([
      ['Review', 5400],
      ['Mail', 1800],
    ]);
    expect(t.byTag).toEqual([
      { tag: 'review', seconds: 5400 },
      { tag: 'team-a', seconds: 5400 },
      { tag: null, seconds: 1800 },
    ]);
  });
});
