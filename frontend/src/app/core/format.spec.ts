import { formatHm, formatHms } from './format';

describe('format', () => {
  it('shows hours and minutes', () => {
    expect(formatHm(0)).toBe('0:00');
    expect(formatHm(59)).toBe('0:00');
    expect(formatHm(60)).toBe('0:01');
    expect(formatHm(3600 + 5 * 60)).toBe('1:05');
    expect(formatHm(26 * 3600)).toBe('26:00');
    expect(formatHm(-5)).toBe('0:00');
  });

  it('shows seconds for the running timer', () => {
    expect(formatHms(3725)).toBe('1:02:05');
    expect(formatHms(9.9)).toBe('0:00:09');
  });
});
