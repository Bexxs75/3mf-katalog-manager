import { uiSettings } from '../lib/uiSettings';
import { useState } from 'react';
export const SINGLE_KEY_SHORTCUTS_KEY = '3mf-katalog-single-key-shortcuts';
export function useSingleKeyShortcuts() {
  const [enabled, setEnabled] = useState(() => uiSettings.getItem(SINGLE_KEY_SHORTCUTS_KEY) !== 'false');
  return [enabled, (value: boolean) => { uiSettings.setItem(SINGLE_KEY_SHORTCUTS_KEY, String(value)); setEnabled(value); }] as const;
}
