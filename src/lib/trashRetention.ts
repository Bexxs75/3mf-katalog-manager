/** How long trashed items stay restorable before the backend deletes them for good. */
export const TRASH_RETENTION_DAYS = 7;
const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * Whole days (rounded up) a trashed item can still be restored, or `null` when the
 * period is over or `deletedAt` cannot be read. Callers show "expires soon" for `null`
 * instead of a number that might be wrong.
 */
export function trashDaysLeft(deletedAt: string | null, now: number = Date.now()): number | null {
  if (!deletedAt) return null;
  const deleted = new Date(deletedAt).getTime();
  if (Number.isNaN(deleted)) return null;
  const left = Math.ceil((deleted + TRASH_RETENTION_DAYS * DAY_MS - now) / DAY_MS);
  return left >= 1 ? Math.min(left, TRASH_RETENTION_DAYS) : null;
}
