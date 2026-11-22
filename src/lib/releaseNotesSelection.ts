export const LAST_SEEN_VERSION_KEY = '3mf-katalog-last-seen-version';

/** Selection deliberately treats a prerelease as its release core. */
export function versionCore(value: string | null): bigint[] | null {
  const match = value?.match(/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:0|[1-9]\d*|\d*[a-zA-Z-][\da-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][\da-zA-Z-]*))*)?(?:\+[\da-zA-Z-]+(?:\.[\da-zA-Z-]+)*)?$/);
  return match && match[0] === value ? match.slice(1, 4).map(BigInt) : null;
}
/** Notes are written for the final x.y.z, so a pre-release (0.18.6-1) looks up its base entry; the full version stays the release identity. */
export function releaseNoteKey(value: string): string {
  const core = versionCore(value);
  return core ? core.join('.') : value;
}
export function compareVersions(a: string, b: string): number {
  const left = versionCore(a), right = versionCore(b);
  if (!left || !right) return 0;
  for (let i = 0; i < 3; i++) if (left[i] !== right[i]) return left[i] > right[i] ? 1 : -1;
  return 0;
}
export function selectReleaseVersions(versions: string[], current: string, marker: string | null, fresh: boolean): string[] {
  if (fresh || !versionCore(current)) return [];
  const validMarker = versionCore(marker);
  return versions.filter(v => versionCore(v) && compareVersions(v, current) <= 0 &&
    (validMarker ? compareVersions(v, marker!) > 0 : compareVersions(v, current) === 0))
    .sort((a, b) => compareVersions(b, a));
}
export function seenVersionOnClose(current: string, stored: string | null): string {
  return !versionCore(stored) || compareVersions(current, stored!) > 0 ? current : stored!;
}
