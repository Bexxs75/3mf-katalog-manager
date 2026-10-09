import { renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { LanguageProvider, useT } from './LanguageContext';

afterEach(() => vi.restoreAllMocks());
it.each([
  ['de', 'MacIntel', 'Im Finder anzeigen', 'Im Finder öffnen'],
  ['en', 'MacIntel', 'Show in Finder', 'Open in Finder'],
  ['es', 'MacIntel', 'Mostrar en el Finder', 'Abrir en el Finder'],
  ['fr', 'MacIntel', 'Afficher dans le Finder', 'Ouvrir dans le Finder'],
  ['de', 'Win32', 'Im Explorer anzeigen', 'Im Explorer öffnen'],
  ['en', 'Win32', 'Show in Explorer', 'Open in Explorer'],
  ['es', 'Win32', 'Mostrar en el Explorador', 'Abrir en el Explorador'],
  ['fr', 'Win32', "Afficher dans l’Explorateur", "Ouvrir dans l’Explorateur"],
  ['de', 'Linux', 'Im Dateimanager anzeigen', 'Im Dateimanager öffnen'],
  ['en', 'unknown', 'Show in file manager', 'Open in file manager'],
])('uses %s file manager labels on %s', (language, platform, show, open) => {
  vi.spyOn(navigator, 'platform', 'get').mockReturnValue(platform);
  localStorage.setItem('3mf-katalog-language', language);
  const { result } = renderHook(() => useT(), {wrapper: LanguageProvider});
  expect(result.current('showInFileManager')).toBe(show);
  expect(result.current('viewerOpenFolder')).toBe(open);
});

it.each([
  [{userAgentData: {platform: 'macOS'}, platform: 'Win32'}, 'Im Finder anzeigen'],
  [{userAgentData: {platform: 'Windows'}, platform: 'MacIntel'}, 'Im Explorer anzeigen'],
  [{platform: '', userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)'}, 'Im Explorer anzeigen'],
  [{platform: '', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)'}, 'Im Finder anzeigen'],
  [{platform: '', userAgent: 'Mozilla/5.0 (X11; Linux x86_64)'}, 'Im Dateimanager anzeigen'],
])('uses navigator platform precedence and user agent fallback (%j)', (values, expected) => {
  vi.stubGlobal('navigator', values);
  try {
    localStorage.setItem('3mf-katalog-language', 'de');
    const { result } = renderHook(() => useT(), {wrapper: LanguageProvider});
    expect(result.current('showInFileManager')).toBe(expected);
  } finally { vi.unstubAllGlobals(); }
});
