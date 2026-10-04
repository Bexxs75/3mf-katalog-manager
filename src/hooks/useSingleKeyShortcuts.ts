import { useState } from 'react';
export const SINGLE_KEY_SHORTCUTS_KEY = '3mf-katalog-single-key-shortcuts';
export function useSingleKeyShortcuts() {
  const [enabled, setEnabled] = useState(() => localStorage.getItem(SINGLE_KEY_SHORTCUTS_KEY) !== 'false');
  return [enabled, (value: boolean) => { localStorage.setItem(SINGLE_KEY_SHORTCUTS_KEY, String(value)); setEnabled(value); }] as const;
}
