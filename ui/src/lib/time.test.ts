import { describe, expect, it } from 'vitest';

import { dayEnabled, minutesToTime, timeToMinutes, toggleDay } from './time';

describe('times', () => {
  it('converts both ways', () => {
    expect(minutesToTime(0)).toBe('00:00');
    expect(minutesToTime(9 * 60 + 5)).toBe('09:05');
    expect(minutesToTime(23 * 60 + 59)).toBe('23:59');
    expect(timeToMinutes('09:05')).toBe(545);
    expect(timeToMinutes('23:59')).toBe(1439);
  });

  it('treats blank as no time', () => {
    expect(minutesToTime(null)).toBe('');
    expect(timeToMinutes('')).toBeNull();
  });

  it('refuses impossible times', () => {
    expect(timeToMinutes('24:00')).toBeNull();
    expect(timeToMinutes('10:60')).toBeNull();
    expect(timeToMinutes('soon')).toBeNull();
  });
});

describe('days', () => {
  it('reads each day from its bit', () => {
    expect(dayEnabled(0b0000001, 0)).toBe(true);
    expect(dayEnabled(0b0000001, 1)).toBe(false);
    expect(dayEnabled(0b1000000, 6)).toBe(true);
  });

  it('switches one day without touching the others', () => {
    expect(toggleDay(0b0000101, 1)).toBe(0b0000111);
    expect(toggleDay(0b0000111, 1)).toBe(0b0000101);
  });
});
