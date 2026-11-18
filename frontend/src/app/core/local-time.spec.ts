import {
  addDays,
  combine,
  fromLocalInput,
  localDate,
  monthGrid,
  monthRange,
  parseHm,
  relativeDayTime,
  splitLocal,
  startOfDay,
  toLocalInput,
  weekRange,
} from './local-time';

// Expectations are built from local Dates, so the tests hold in any time zone.
describe('local time', () => {
  it('reads typed times in 24-hour form without seconds', () => {
    const cases: [string, string | null][] = [
      ['9', '09:00'],
      ['09', '09:00'],
      ['930', '09:30'],
      ['0930', '09:30'],
      ['1745', '17:45'],
      ['9:30', '09:30'],
      ['9.30', '09:30'],
      ['9:3', '09:30'],
      [' 17:05 ', '17:05'],
      ['23:59', '23:59'],
      ['0:00', '00:00'],
      ['24:00', null],
      ['12:60', null],
      ['5 pm', null],
      ['17:45:10', null],
      ['', null],
    ];
    for (const [text, expected] of cases) expect(parseHm(text), text).toBe(expected);
    expect(cases.length).toBe(16);
  });

  it('combines a date with a typed time and splits an instant', () => {
    expect(combine('2026-11-17', '930')?.getTime()).toBe(new Date(2026, 10, 17, 9, 30).getTime());
    expect(combine('2026-11-17', '25:00')).toBeNull();
    expect(combine('', '09:30')).toBeNull();
    expect(splitLocal(new Date(2026, 10, 17, 18, 5).toISOString())).toEqual({
      date: '2026-11-17',
      time: '18:05',
    });
  });

  it('finds the week of a date for both week starts', () => {
    // 2026-11-18 is a Wednesday.
    expect(weekRange('2026-11-18', 'monday')).toEqual(['2026-11-16', '2026-11-22']);
    expect(weekRange('2026-11-18', 'sunday')).toEqual(['2026-11-15', '2026-11-21']);
    expect(weekRange('2026-11-16', 'monday')).toEqual(['2026-11-16', '2026-11-22']);
    expect(weekRange('2026-11-22', 'monday')).toEqual(['2026-11-16', '2026-11-22']);
    expect(weekRange('2026-11-22', 'sunday')).toEqual(['2026-11-22', '2026-11-28']);
  });

  it('finds months and lays out whole weeks', () => {
    expect(monthRange('2026-02-14')).toEqual(['2026-02-01', '2026-02-28']);
    expect(monthRange('2026-11-30')).toEqual(['2026-11-01', '2026-11-30']);
    const grid = monthGrid('2026-11-18', 'monday');
    expect(grid.length).toBe(6); // 26 Oct – 6 Dec
    expect(grid[0][0]).toBe('2026-10-26');
    expect(grid[5][6]).toBe('2026-12-06');
    expect(grid.every((w) => w.length === 7)).toBe(true);
    expect(monthGrid('2026-11-18', 'sunday')[0][0]).toBe('2026-11-01');
  });

  it('round-trips datetime-local values', () => {
    const d = new Date(2026, 10, 17, 9, 5);
    expect(toLocalInput(d.toISOString())).toBe('2026-11-17T09:05');
    expect(fromLocalInput('2026-11-17T09:05')?.getTime()).toBe(d.getTime());
    expect(fromLocalInput('')).toBeNull();
    expect(fromLocalInput('17.11.2026 09:05')).toBeNull();
  });

  it('moves through days across month ends', () => {
    expect(addDays('2026-11-30', 1)).toBe('2026-12-01');
    expect(addDays('2026-03-01', -1)).toBe('2026-02-28');
    expect(localDate(startOfDay('2026-11-17'))).toBe('2026-11-17');
    expect(startOfDay('2026-11-17').getHours()).toBe(0);
  });

  it('labels today and yesterday', () => {
    const now = new Date(2026, 10, 17, 12, 0);
    expect(relativeDayTime(new Date(2026, 10, 17, 9, 12).toISOString(), now)).toBe('today 09:12');
    expect(relativeDayTime(new Date(2026, 10, 16, 9, 12).toISOString(), now)).toBe(
      'yesterday 09:12',
    );
    expect(relativeDayTime(new Date(2026, 10, 10, 9, 12).toISOString(), now)).toBe(
      'Tue 10 Nov 09:12',
    );
  });
});
