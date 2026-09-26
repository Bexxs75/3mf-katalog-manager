import { useCallback, useState } from 'react';

export type DisplayPreference = 'thumbnail' | 'render';

const STORAGE_KEY = '3mf-katalog-display-preference';

/**
 * Whether cards and detail page prefer the embedded file image ("thumbnail")
 * or the rendered 3D view ("render"), stored in localStorage.
 * Default "thumbnail" so nothing changes for existing users.
 */
export function useDisplayPreference() {
  const [preference, setPreferenceState] = useState<DisplayPreference>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === 'render' ? 'render' : 'thumbnail';
  });

  const setPreference = useCallback((next: DisplayPreference) => {
    setPreferenceState(next);
    localStorage.setItem(STORAGE_KEY, next);
  }, []);

  return { preference, setPreference };
}
