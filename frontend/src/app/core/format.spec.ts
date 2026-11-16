import { formatHm, formatHms, formatHours, formatHuman } from './format';

describe('format', () => {
  it('shows hours and minutes', () => {
    expect(formatHm(0)).toBe('0:00');
    expect(formatHm(59)).toBe('0:00');
    expect(formatHm(60)).toBe('0:01');
    expect(formatHm(3600 + 5 * 60)).toBe('1:05');
    expect(formatHm(26 * 3600)).toBe('26:00');
    expect(formatHm(-5)).toBe('0:00');
  });

  it('shows report hours with one decimal', () => {
    expect(formatHours(0)).toBe('0.0 h');
    expect(formatHours(5400)).toBe('1.5 h');
    expect(formatHours(3600 * 7 + 59 * 60)).toBe('8.0 h');
  });

  it('writes durations for sentences', () => {
    expect(formatHuman(45 * 60)).toBe('45 min');
    expect(formatHuman(2 * 3600)).toBe('2 h');
    expect(formatHuman(23 * 3600 + 40 * 60 + 59)).toBe('23 h 40 min');
  });

  it('shows seconds for the running timer', () => {
    expect(formatHms(3725)).toBe('1:02:05');
    expect(formatHms(9.9)).toBe('0:00:09');
  });
});
