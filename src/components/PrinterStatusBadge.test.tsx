import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { PrinterStatusBadge, printerStatus, printerStatusColors } from './PrinterStatusBadge';
import type { Printer, PrinterConnection } from '../types';

const printer = { id: '1', kind: 'filament' } as Printer;
const base = {
  printerId: '1', kind: 'moonraker', address: '192.168.1.60', baseUrl: null, remoteVersion: null, connectedSince: 1,
  lastSyncedAt: null, lastError: null, errorSince: null, paused: false, clockOffsetS: 0,
} as PrinterConnection;

describe('printerStatus', () => {
  it.each([
    ['ok', 'pmConnected'],
    ['reconnecting', 'pmReconnecting'],
    ['offline', 'pmOffline'],
    ['error', 'pmConnectionError'],
    ['paused', 'pmConnectionError'],
  ] as const)('maps the backend status %s', (status, key) => {
    // The stored error fields say the opposite on purpose: the derived status decides.
    expect(printerStatus(printer, true, { ...base, status, lastError: status === 'ok' ? 'timeout' : null })).toBe(key);
  });

  it('falls back to the error fields when there is no derived status', () => {
    expect(printerStatus(printer, true, base)).toBe('pmConnected');
    expect(printerStatus(printer, true, { ...base, lastError: 'timeout' })).toBe('pmConnectionError');
    expect(printerStatus(printer, true, { ...base, paused: true })).toBe('pmConnectionError');
  });

  it('is inactive without connection, with the switch off or for resin', () => {
    expect(printerStatus(printer, true, undefined)).toBe('pmInactive');
    expect(printerStatus(printer, false, { ...base, status: 'ok' })).toBe('pmInactive');
    expect(printerStatus({ ...printer, kind: 'resin' } as Printer, true, { ...base, status: 'ok' })).toBe('pmInactive');
  });

  it('names every state in words, so colour is never the only sign', () => {
    localStorage.setItem('3mf-katalog-language', 'de');
    const seen: string[] = [];
    for (const key of Object.keys(printerStatusColors) as Array<keyof typeof printerStatusColors>) {
      const { unmount } = render(<LanguageProvider><PrinterStatusBadge status={key} /></LanguageProvider>);
      seen.push(screen.getByText(/\S/, { selector: 'span' }).textContent ?? '');
      unmount();
    }
    expect(new Set(seen).size).toBe(seen.length);
    expect(seen).toEqual(expect.arrayContaining(['Verbinde neu', 'Nicht erreichbar', 'Verbunden', 'Inaktiv', 'Fehler']));
  });
});
