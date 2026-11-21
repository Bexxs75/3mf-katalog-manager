import { beforeEach } from 'vitest';
import { resetUiSettings } from '../lib/uiSettings';
import '@testing-library/jest-dom/vitest';

beforeEach(resetUiSettings);

// jsdom has no matchMedia - useTheme.ts (system preference) needs it.
if (!window.matchMedia) {
  window.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  }) as unknown as MediaQueryList;
}
