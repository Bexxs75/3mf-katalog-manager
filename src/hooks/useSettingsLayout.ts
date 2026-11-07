import { useEffect, useState } from 'react';
import { resolveSettingsLayout, type SettingsLayout } from '../lib/settingsLayout';

/** The settings panel's layout for the current window width; see `resolveSettingsLayout` for the rules. */
export function useSettingsLayout(forceOverlay: boolean): SettingsLayout {
  const [windowWidth, setWindowWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    const update = () => setWindowWidth(window.innerWidth);
    update();
    window.addEventListener('resize', update);
    return () => window.removeEventListener('resize', update);
  }, []);
  return resolveSettingsLayout(windowWidth, forceOverlay);
}
