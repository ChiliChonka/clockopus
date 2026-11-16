import {
  addDays,
  fromLocalInput,
  localDate,
  relativeDayTime,
  startOfDay,
  toLocalInput,
} from './local-time';

// Expectations are built from local Dates, so the tests hold in any time zone.
describe('local time', () => {
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
