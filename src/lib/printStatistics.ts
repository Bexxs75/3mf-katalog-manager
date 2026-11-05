import { printStatusGroup, type PrintStatusGroup } from './printStatusGroups';
import { PRINT_FAILURE_REASONS } from './printFailureReasons';
import type { PrintStatRow } from '../types';

/** Pure functions behind the statistics card. Times are Unix seconds; calendar days and months are those of `timeZone`. */

export type StatRange = '30d' | '12m' | 'all';
/** The three classes of the bars and key figures; every group but "succeeded" and "cancelled" counts as failed. */
export type StatOutcome = 'succeeded' | 'cancelled' | 'failed';

/** Rows of the 30-day view are summed in blocks of this many days. */
export const DAY_BUCKET = 5;
export const RANGE_DAYS = 30;
export const RANGE_MONTHS = 12;

/** The finer group of a print (see printStatusGroups.ts). */
export function groupOf(row: Pick<PrintStatRow, 'rawStatus' | 'failureReason'>): PrintStatusGroup {
  return printStatusGroup(row.rawStatus, row.failureReason);
}

/** Succeeded, cancelled or failed: the same classes the history table shows as "Result". */
export function outcomeOf(row: Pick<PrintStatRow, 'rawStatus' | 'failureReason'>): StatOutcome {
  const group = groupOf(row);
  return group === 'succeeded' || group === 'cancelled' ? group : 'failed';
}

const formatters = new Map<string, Intl.DateTimeFormat>();
function formatterFor(timeZone: string | undefined): Intl.DateTimeFormat {
  const key = timeZone ?? '';
  let f = formatters.get(key);
  if (!f) {
    f = new Intl.DateTimeFormat('en-US', { timeZone, year: 'numeric', month: 'numeric', day: 'numeric', calendar: 'gregory', numberingSystem: 'latn' });
    formatters.set(key, f);
  }
  return f;
}

/** Calendar date of a moment in a time zone. */
function civil(seconds: number, timeZone: string | undefined): { year: number; month: number; day: number } {
  const out = { year: 1970, month: 1, day: 1 };
  for (const part of formatterFor(timeZone).formatToParts(seconds * 1000)) {
    if (part.type === 'year' || part.type === 'month' || part.type === 'day') out[part.type] = Number(part.value);
  }
  return out;
}

/** Days since 1970-01-01 of the calendar day in the zone. Whole numbers, so a 23 or 25 hour day (summer time) is one day. */
export function dayNumber(seconds: number, timeZone?: string): number {
  const { year, month, day } = civil(seconds, timeZone);
  return Math.round(Date.UTC(year, month - 1, day) / 86_400_000);
}

/** Months since year 0 (year * 12 + month index) of the moment in the zone. */
export function monthNumber(seconds: number, timeZone?: string): number {
  const { year, month } = civil(seconds, timeZone);
  return year * 12 + (month - 1);
}

export interface Counts {
  succeeded: number;
  cancelled: number;
  failed: number;
  total: number;
}

const emptyCounts = (): Counts => ({ succeeded: 0, cancelled: 0, failed: 0, total: 0 });
function add(counts: Counts, row: PrintStatRow) {
  counts[outcomeOf(row)] += 1;
  counts.total += 1;
}

/** Rows inside the range; "30d" is today and the 29 days before it, "12m" this month and the 11 before it. */
export function inRange(rows: PrintStatRow[], range: StatRange, now: number, timeZone?: string): PrintStatRow[] {
  if (range === 'all') return rows;
  if (range === '30d') {
    const today = dayNumber(now, timeZone);
    return rows.filter(r => { const age = today - dayNumber(r.endedAt, timeZone); return age >= 0 && age < RANGE_DAYS; });
  }
  const current = monthNumber(now, timeZone);
  return rows.filter(r => { const age = current - monthNumber(r.endedAt, timeZone); return age >= 0 && age < RANGE_MONTHS; });
}

export interface MonthBucket extends Counts {
  /** "2026-03" */
  key: string;
  year: number;
  /** 1 to 12 */
  month: number;
}

const monthKey = (n: number) => `${Math.floor(n / 12)}-${String((n % 12) + 1).padStart(2, '0')}`;

/**
 * One bucket per calendar month up to the current one, oldest first, empty months included.
 * `months` limits the span (12 for "12 months"); `null` runs from the earliest row. Rows in the
 * future (a clock that ran ahead) count in the current month.
 */
export function byMonth(rows: PrintStatRow[], now: number, timeZone: string | undefined, months: number | null): MonthBucket[] {
  const current = monthNumber(now, timeZone);
  const of = (r: PrintStatRow) => Math.min(current, monthNumber(r.endedAt, timeZone));
  const first = months !== null ? current - months + 1 : rows.reduce((m, r) => Math.min(m, of(r)), current);
  const buckets: MonthBucket[] = [];
  for (let n = first; n <= current; n++) buckets.push({ key: monthKey(n), year: Math.floor(n / 12), month: (n % 12) + 1, ...emptyCounts() });
  for (const row of rows) {
    const index = of(row) - first;
    if (index >= 0 && index < buckets.length) add(buckets[index], row);
  }
  return buckets;
}

export interface DayBucket extends Counts {
  /** First and last calendar day (day numbers, see `dayNumber`) of the block. */
  startDay: number;
  endDay: number;
}

/** The last `days` days in blocks of `step` days, oldest first; the newest block ends today. */
export function byDays(rows: PrintStatRow[], now: number, timeZone: string | undefined, days = RANGE_DAYS, step = DAY_BUCKET): DayBucket[] {
  const today = dayNumber(now, timeZone);
  const count = Math.ceil(days / step);
  const buckets: DayBucket[] = [];
  for (let i = count - 1; i >= 0; i--) buckets.push({ startDay: today - (i + 1) * step + 1, endDay: today - i * step, ...emptyCounts() });
  for (const row of rows) {
    const age = today - dayNumber(row.endedAt, timeZone);
    if (age < 0 || age >= count * step) continue;
    add(buckets[count - 1 - Math.floor(age / step)], row);
  }
  return buckets;
}

export interface Totals extends Counts {
  /** Whole percent of all prints, or null without prints. */
  successRate: number | null;
  printSeconds: number;
  /** Grams of confirmed prints only. */
  grams: number;
  /** Prints that carry no grams (not confirmed, or without usage). */
  withoutGrams: number;
}

export function totals(rows: PrintStatRow[]): Totals {
  const out: Totals = { ...emptyCounts(), successRate: null, printSeconds: 0, grams: 0, withoutGrams: 0 };
  for (const row of rows) {
    add(out, row);
    out.printSeconds += row.printDurationS;
    if (row.grams != null && row.state === 'confirmed') out.grams += row.grams; else out.withoutGrams += 1;
  }
  out.successRate = out.total > 0 ? Math.round((100 * out.succeeded) / out.total) : null;
  return out;
}

export interface ReasonRow<K> {
  key: K;
  count: number;
}

export interface ReasonsBreakdown {
  /** Prints that did not succeed; the shares refer to this number. */
  total: number;
  /** By what the printer reported, groups with prints only, in the order of the group list. */
  byStatus: ReasonRow<PrintStatusGroup>[];
  /** By the reason the user gave, most frequent first, "no reason" (null) always last. */
  byReason: ReasonRow<string | null>[];
}

/** Why prints that did not succeed ended: what the printer said, and what the user added. */
export function reasonsBreakdown(rows: PrintStatRow[]): ReasonsBreakdown {
  const bad = rows.filter(r => outcomeOf(r) !== 'succeeded');
  const status = new Map<PrintStatusGroup, number>();
  const reason = new Map<string | null, number>();
  for (const row of bad) {
    // What the printer reported, without the user's reason that overrides the group.
    const reported = printStatusGroup(row.rawStatus, null);
    status.set(reported, (status.get(reported) ?? 0) + 1);
    const given = row.failureReason || null;
    reason.set(given, (reason.get(given) ?? 0) + 1);
  }
  const known: readonly string[] = PRINT_FAILURE_REASONS;
  const rank = (code: string) => { const i = known.indexOf(code); return i < 0 ? known.length : i; };
  const byReason = [...reason.entries()]
    .filter((e): e is [string, number] => e[0] !== null)
    .sort((a, b) => b[1] - a[1] || rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]))
    .map(([key, count]) => ({ key: key as string | null, count }));
  const none = reason.get(null);
  if (none) byReason.push({ key: null, count: none });
  // The order of the status groups comes from the shared list; a status the list lacks is "other".
  const groupOrder = ['succeeded', 'cancelled', 'failed', 'printer_error', 'interrupted', 'other'] as const;
  const byStatus = groupOrder.filter(g => status.has(g)).map(g => ({ key: g as PrintStatusGroup, count: status.get(g) ?? 0 }));
  return { total: bad.length, byStatus, byReason };
}

export interface MaterialGrams {
  /** null: the print has no material recorded. */
  material: string | null;
  grams: number;
}

/** Grams per material, most first; only confirmed prints carry grams. */
export function materialGrams(rows: PrintStatRow[]): MaterialGrams[] {
  const sums = new Map<string | null, number>();
  for (const row of rows) {
    if (row.grams == null || row.state !== 'confirmed') continue;
    const material = row.material?.trim() || null;
    sums.set(material, (sums.get(material) ?? 0) + row.grams);
  }
  return [...sums.entries()]
    .map(([material, grams]) => ({ material, grams }))
    .sort((a, b) => b.grams - a.grams || (a.material ?? '~').localeCompare(b.material ?? '~'));
}
