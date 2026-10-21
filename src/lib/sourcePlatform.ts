// Display names for the stable platform keys the backend stores. The key, not
// the name, is persisted, so names can change without a data migration.
const PLATFORM_NAMES: Record<string, string> = {
  makerworld: 'Makerworld',
};

// Unknown keys (for example from a newer backend) are shown as-is rather than hidden.
export function platformDisplayName(key: string): string {
  return Object.prototype.hasOwnProperty.call(PLATFORM_NAMES, key) ? PLATFORM_NAMES[key] : key;
}
