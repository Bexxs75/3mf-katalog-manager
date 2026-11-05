import gold from './printStatusGroups.json';

/** Group of a print in the history. The same table is checked in Rust. */
export type PrintStatusGroup = (typeof gold.groups)[number];

const STATUS_GROUPS: Readonly<Record<string, PrintStatusGroup>> = gold.statusGroups;

/**
 * The group of a print from the status its printer reported. A reason set by the
 * user overrides it: `intentional` stays "cancelled", every other reason counts as "failed".
 */
export function printStatusGroup(rawStatus: string, failureReason: string | null): PrintStatusGroup {
  if (failureReason) return failureReason === 'intentional' ? gold.intentionalGroup : gold.reasonGroup;
  const key = rawStatus.trim().toLowerCase();
  return Object.prototype.hasOwnProperty.call(STATUS_GROUPS, key) ? STATUS_GROUPS[key] : gold.otherGroup;
}
