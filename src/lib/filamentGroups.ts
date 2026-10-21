import type { FilamentSpool } from '../types';
import { filamentStockStatus, type FilamentStockStatus } from './filamentStatus';

/** Where the spools of a group are: on a shelf or loaded into one printer unit. */
export type SpoolGroupPlace =
  | { kind: 'storage'; location: string | null }
  | { kind: 'unit'; unitId: string; label: string; /** 1-based, ascending. */ slots: number[] };

export interface SpoolGroup {
  /** Id of the first member: stable while that spool exists; React key and expanded state. */
  key: string;
  /** Members in input order. */
  spools: FilamentSpool[];
  /** First member: its text fields stand for the whole group. */
  first: FilamentSpool;
  place: SpoolGroupPlace;
  remainingG: number;
  originalG: number;
  /** Remaining / original of the whole group (0 when there is no original amount). */
  ratio: number;
  /** Status by the group's total, with the same thresholds as a single spool. */
  status: FilamentStockStatus;
  /** Members with nothing left. */
  emptyCount: number;
}

const round = (n: number) => Math.round(n * 10) / 10;

/** Colors a spool carries, first one first; empty when it only has a color name. */
export function spoolColorList(spool: Pick<FilamentSpool, 'colorHex' | 'colorHexes'>): string[] {
  const list = spool.colorHexes?.length ? spool.colorHexes : spool.colorHex ? [spool.colorHex] : [];
  return list.map((hex) => hex.toLowerCase());
}

const nameKey = (name: string | null) => (name ?? '').trim().toLowerCase();

/**
 * Same color? With a color value on both spools the values decide (the full list in the
 * same order for multi-color spools); otherwise the color names, ignoring case and
 * surrounding spaces.
 */
export function sameSpoolColor(a: FilamentSpool, b: FilamentSpool): boolean {
  const listA = spoolColorList(a);
  const listB = spoolColorList(b);
  if (listA.length > 0 && listB.length > 0) {
    return listA.length === listB.length && listA.every((hex, i) => hex === listB[i]);
  }
  return nameKey(a.color) === nameKey(b.color);
}

/** Everything but the color: equal text, equal place, equal diameter. */
function placeKey(spool: FilamentSpool): string {
  const where = spool.unitId !== null ? `unit:${spool.unitId}` : `shelf:${spool.location ?? ''}`;
  // JSON keeps the parts apart even when a text contains the separator.
  return JSON.stringify([spool.kind, spool.material, spool.manufacturer ?? '', where, spool.diameterMm]);
}

function describe(members: FilamentSpool[], unitLabel: (unitId: string) => string, key: string): SpoolGroup {
  const first = members[0];
  const remainingG = round(members.reduce((sum, s) => sum + s.remainingWeightG, 0));
  const originalG = round(members.reduce((sum, s) => sum + s.originalWeightG, 0));
  const place: SpoolGroupPlace =
    first.unitId !== null
      ? {
          kind: 'unit',
          unitId: first.unitId,
          label: unitLabel(first.unitId),
          slots: members.flatMap((s) => (s.slotIndex === null ? [] : [s.slotIndex + 1])).sort((x, y) => x - y),
        }
      : { kind: 'storage', location: first.location };
  return {
    key,
    spools: members,
    first,
    place,
    remainingG,
    originalG,
    ratio: originalG > 0 ? remainingG / originalG : 0,
    status: filamentStockStatus({ originalWeightG: originalG, remainingWeightG: remainingG }),
    emptyCount: members.filter((s) => s.remainingWeightG <= 0).length,
  };
}

/**
 * Groups equal spools: same material text, manufacturer, diameter, color (see
 * `sameSpoolColor`) and place (shelf location, or the printer unit for loaded spools,
 * which never mix with shelf spools). Colors are not transitive (a spool without a color
 * value matches by name only), so a spool joins a group only when it matches every member.
 * The result is sorted by the group's remaining share, lowest first, so empty and low
 * groups come first; equal shares keep the input order.
 */
export function groupFilaments(spools: FilamentSpool[], unitLabel: (unitId: string) => string = () => ''): SpoolGroup[] {
  const buckets = new Map<string, FilamentSpool[][]>();
  for (const spool of spools) {
    const key = placeKey(spool);
    const groups = buckets.get(key) ?? [];
    const target = groups.find((members) => members.every((m) => sameSpoolColor(m, spool)));
    if (target) target.push(spool);
    else groups.push([spool]);
    buckets.set(key, groups);
  }
  const result: SpoolGroup[] = [];
  for (const groups of buckets.values()) {
    for (const members of groups) result.push(describe(members, unitLabel, members[0].id));
  }
  const order = new Map(spools.map((s, i) => [s.id, i]));
  return result.sort((a, b) => a.ratio - b.ratio || (order.get(a.first.id) ?? 0) - (order.get(b.first.id) ?? 0));
}

/** The "Single" view: every spool is a group of its own, in the given order. */
export function singleSpoolGroups(spools: FilamentSpool[], unitLabel: (unitId: string) => string = () => ''): SpoolGroup[] {
  return spools.map((spool) => describe([spool], unitLabel, spool.id));
}
