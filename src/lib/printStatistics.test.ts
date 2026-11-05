import { describe, expect, it } from 'vitest';
import gold from './printStatusGroups.json';
import type { PrintStatRow } from '../types';
import { byDays, byMonth, dayNumber, groupOf, inRange, materialGrams, monthNumber, outcomeOf, reasonsBreakdown, totals } from './printStatistics';

const at = (iso: string) => Date.parse(iso) / 1000;
const row = (over: Partial<PrintStatRow> = {}): PrintStatRow => ({
  printerId: 'p1', endedAt: at('2026-03-10T12:00:00Z'), outcome: 'completed', rawStatus: 'completed', printDurationS: 3600,
  grams: null, usedMm: 0, material: null, state: 'confirmed', autoClosed: false, failureReason: null, ...over,
});

describe('groupOf / outcomeOf', () => {
  it.each(gold.cases)('uses the shared table: $rawStatus / $reason', c => {
    expect(groupOf({ rawStatus: c.rawStatus, failureReason: c.reason })).toBe(c.group);
  });
  it('folds the finer groups into three classes', () => {
    const o = (rawStatus: string, failureReason: string | null = null) => outcomeOf({ rawStatus, failureReason });
    expect(o('completed')).toBe('succeeded');
    expect(o('cancelled')).toBe('cancelled');
    expect(o('cancelled', 'intentional')).toBe('cancelled');
    expect(o('cancelled', 'clog')).toBe('failed');
    expect(o('error')).toBe('failed');
    expect(o('interrupted')).toBe('failed');
    expect(o('something_new')).toBe('failed');
  });
});

describe('time zones and month borders', () => {
  const lastSecondOfMarchUtc = at('2026-03-31T23:59:59Z');
  it('puts the same moment into different months in different zones', () => {
    expect(monthNumber(lastSecondOfMarchUtc, 'UTC')).toBe(2026 * 12 + 2);
    expect(monthNumber(lastSecondOfMarchUtc, 'Europe/Berlin')).toBe(2026 * 12 + 3); // 01:59 on 1 April
    expect(monthNumber(at('2026-04-01T05:00:00Z'), 'America/Los_Angeles')).toBe(2026 * 12 + 2); // 22:00 on 31 March
  });
  it('counts a day as one day across the spring and autumn clock change', () => {
    const tz = 'Europe/Berlin';
    // 23 h day (29 March 2026) and 25 h day (25 October 2026): the next midnight is always the next day.
    expect(dayNumber(at('2026-03-29T22:30:00Z'), tz) - dayNumber(at('2026-03-28T22:30:00Z'), tz)).toBe(2);
    expect(dayNumber(at('2026-03-30T21:59:00Z'), tz)).toBe(dayNumber(at('2026-03-30T00:30:00Z'), tz));
    expect(dayNumber(at('2026-10-25T21:59:00Z'), tz)).toBe(dayNumber(at('2026-10-25T00:30:00Z'), tz));
    expect(dayNumber(at('2026-10-25T23:00:00Z'), tz) - dayNumber(at('2026-10-24T22:00:00Z'), tz)).toBe(1);
  });
  it('byMonth assigns a print at the month border to the month of the zone', () => {
    const rows = [row({ endedAt: lastSecondOfMarchUtc })];
    const now = at('2026-04-15T10:00:00Z');
    const utc = byMonth(rows, now, 'UTC', 3);
    const berlin = byMonth(rows, now, 'Europe/Berlin', 3);
    expect(utc.map(b => [b.key, b.total])).toEqual([['2026-02', 0], ['2026-03', 1], ['2026-04', 0]]);
    expect(berlin.map(b => [b.key, b.total])).toEqual([['2026-02', 0], ['2026-03', 0], ['2026-04', 1]]);
  });
  it('byMonth spans a year change with empty months, oldest first', () => {
    const buckets = byMonth([row({ endedAt: at('2025-11-20T12:00:00Z') }), row({ endedAt: at('2026-02-05T12:00:00Z'), rawStatus: 'cancelled' })], at('2026-02-20T12:00:00Z'), 'UTC', null);
    expect(buckets.map(b => [b.key, b.succeeded, b.cancelled])).toEqual([['2025-11', 1, 0], ['2025-12', 0, 0], ['2026-01', 0, 0], ['2026-02', 0, 1]]);
  });
  it('byMonth with 12 months gives exactly 12 buckets and drops older rows', () => {
    const rows = [row({ endedAt: at('2025-03-31T12:00:00Z') }), row({ endedAt: at('2025-04-01T12:00:00Z') })];
    const buckets = byMonth(rows, at('2026-03-15T12:00:00Z'), 'UTC', 12);
    expect(buckets).toHaveLength(12);
    expect(buckets[0].key).toBe('2025-04');
    expect(buckets.reduce((n, b) => n + b.total, 0)).toBe(1);
  });
  it('a print from the future counts in the current month', () => {
    const buckets = byMonth([row({ endedAt: at('2026-06-01T00:00:00Z') })], at('2026-03-15T12:00:00Z'), 'UTC', 2);
    expect(buckets.map(b => b.total)).toEqual([0, 1]);
  });
  it('byDays: 6 blocks of 5 days, the newest ends today, across the clock change', () => {
    const tz = 'Europe/Berlin';
    const now = at('2026-03-30T10:00:00Z'); // Monday, after the change on 29 March
    const blocks = byDays([
      row({ endedAt: at('2026-03-30T08:00:00Z') }), // today
      row({ endedAt: at('2026-03-26T09:00:00Z') }), // 4 days back: still the newest block
      row({ endedAt: at('2026-03-25T09:00:00Z') }), // 5 days back: the block before
      row({ endedAt: at('2026-03-01T09:00:00Z'), rawStatus: 'error' }), // 29 days back: the oldest block
      row({ endedAt: at('2026-02-28T09:00:00Z') }), // 30 days back: out
    ], now, tz);
    expect(blocks).toHaveLength(6);
    expect(blocks.map(b => b.total)).toEqual([1, 0, 0, 0, 1, 2]);
    expect(blocks[5].endDay).toBe(dayNumber(now, tz));
    expect(blocks[0].startDay).toBe(dayNumber(now, tz) - 29);
    expect(blocks[0].failed).toBe(1);
  });
  it('inRange keeps today and the 29 days before it, this month and the 11 before it', () => {
    const now = at('2026-03-30T10:00:00Z');
    const rows = [row({ endedAt: at('2026-03-01T00:30:00Z') }), row({ endedAt: at('2026-02-28T00:30:00Z') }), row({ endedAt: at('2025-04-02T12:00:00Z') }), row({ endedAt: at('2025-03-31T12:00:00Z') })];
    expect(inRange(rows, '30d', now, 'UTC')).toHaveLength(1);
    expect(inRange(rows, '12m', now, 'UTC')).toHaveLength(3);
    expect(inRange(rows, 'all', now, 'UTC')).toHaveLength(4);
  });
});

describe('totals', () => {
  it('matches a hand count', () => {
    const rows = [
      row({ grams: 10.5, material: 'PLA', printDurationS: 3600 }),
      row({ grams: 20, material: 'PETG', printDurationS: 1800 }),
      row({ rawStatus: 'cancelled', printDurationS: 600, state: 'ignored' }),
      row({ rawStatus: 'error', printDurationS: 60, state: 'open' }),
      row({ rawStatus: 'cancelled', failureReason: 'clog', printDurationS: 30, grams: 4, state: 'confirmed' }),
      row({ rawStatus: 'cancelled', failureReason: 'intentional', printDurationS: 30 }),
    ];
    const t = totals(rows);
    expect(t).toMatchObject({ total: 6, succeeded: 2, cancelled: 2, failed: 2, printSeconds: 6120, withoutGrams: 3 });
    expect(t.grams).toBeCloseTo(34.5);
    expect(t.successRate).toBe(33);
  });
  it('counts grams of confirmed prints only', () => {
    const t = totals([row({ grams: 99, state: 'open' }), row({ grams: 5 })]);
    expect(t.grams).toBe(5);
    expect(t.withoutGrams).toBe(1);
  });
  it('is empty without prints and has no rate', () => {
    expect(totals([])).toMatchObject({ total: 0, successRate: null, grams: 0, printSeconds: 0, withoutGrams: 0 });
  });
});

describe('reasonsBreakdown', () => {
  const rows = [
    row(),
    row({ rawStatus: 'cancelled', failureReason: 'clog' }),
    row({ rawStatus: 'cancelled', failureReason: 'clog' }),
    row({ rawStatus: 'cancelled', failureReason: 'adhesion' }),
    row({ rawStatus: 'cancelled' }),
    row({ rawStatus: 'error' }),
    row({ rawStatus: 'klippy_shutdown', failureReason: 'intentional' }),
    row({ rawStatus: 'cancelled', failureReason: 'from_a_newer_version' }),
  ];
  it('splits what the printer said from what the user added, "no reason" last', () => {
    const b = reasonsBreakdown(rows);
    expect(b.total).toBe(7);
    expect(b.byStatus).toEqual([{ key: 'cancelled', count: 5 }, { key: 'printer_error', count: 2 }]);
    expect(b.byReason).toEqual([
      { key: 'clog', count: 2 }, { key: 'adhesion', count: 1 }, { key: 'intentional', count: 1 }, { key: 'from_a_newer_version', count: 1 }, { key: null, count: 2 },
    ]);
    expect(b.byReason.reduce((n, r) => n + r.count, 0)).toBe(b.total);
  });
  it('is empty when everything succeeded', () => {
    expect(reasonsBreakdown([row(), row()])).toEqual({ total: 0, byStatus: [], byReason: [] });
  });
});

describe('materialGrams', () => {
  it('sums confirmed grams per material, most first, unknown material as null', () => {
    const out = materialGrams([
      row({ grams: 10, material: 'PLA' }), row({ grams: 5.5, material: 'PLA' }), row({ grams: 30, material: 'PETG' }),
      row({ grams: 7, material: null }), row({ grams: 100, material: 'ABS', state: 'open' }), row({ grams: null, material: 'TPU' }),
    ]);
    expect(out).toEqual([{ material: 'PETG', grams: 30 }, { material: 'PLA', grams: 15.5 }, { material: null, grams: 7 }]);
  });
  it('is empty without grams', () => { expect(materialGrams([row()])).toEqual([]); });
});
