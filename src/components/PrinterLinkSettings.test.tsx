import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrinterLinkSettings } from './PrinterLinkSettings';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { Printer } from '../types';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function link(over: Partial<PrinterLinkState> = {}): PrinterLinkState {
  return {
    enabled: false, connections: [], jobs: [], error: null,
    refresh: vi.fn(), setEnabled: vi.fn().mockResolvedValue(undefined), testConnection: vi.fn(),
    removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(),
    ...over,
  } as PrinterLinkState;
}

const printers: Printer[] = [
  { id: '1', name: 'Sovol SV08', kind: 'filament', units: [] },
  { id: '2', name: 'Werkstatt-Drucker', kind: 'filament', units: [] },
];

describe('PrinterLinkSettings', () => {
  it('switches the printer connection', () => {
    const l = link();
    render(<LanguageProvider><PrinterLinkSettings link={l} printers={printers} /></LanguageProvider>);
    fireEvent.click(screen.getByRole('switch', { name: 'Druckeranbindung' }));
    expect(l.setEnabled).toHaveBeenCalledWith(true);
  });

  it('lists printers with their status when switched on', () => {
    const l = link({
      enabled: true,
      connections: [{ printerId: '1', kind: 'moonraker', address: '192.168.1.60', baseUrl: null, remoteVersion: null,
        connectedSince: 1, lastSyncedAt: 2, lastError: null, errorSince: null, paused: false, clockOffsetS: 0 }],
    });
    render(<LanguageProvider><PrinterLinkSettings link={l} printers={printers} /></LanguageProvider>);
    expect(screen.getByText('Klipper · verbunden')).toBeInTheDocument();
    expect(screen.getByText('nicht angebunden')).toBeInTheDocument();
  });

  it('shows a paused-retest hint instead of "verbunden" for a paused connection without an error', () => {
    const l = link({
      enabled: true,
      connections: [{ printerId: '1', kind: 'moonraker', address: '192.168.1.60', baseUrl: null, remoteVersion: null,
        connectedSince: 1, lastSyncedAt: 2, lastError: null, errorSince: null, paused: true, clockOffsetS: 0 }],
    });
    render(<LanguageProvider><PrinterLinkSettings link={l} printers={printers} /></LanguageProvider>);
    expect(screen.getByText('pausiert – bitte Verbindung neu testen')).toBeInTheDocument();
    expect(screen.queryByText('Klipper · verbunden')).not.toBeInTheDocument();
  });

  it('shows an error message when switching the toggle is rejected', async () => {
    const l = link({ setEnabled: vi.fn().mockRejectedValue('offline') });
    render(<LanguageProvider><PrinterLinkSettings link={l} printers={printers} /></LanguageProvider>);
    fireEvent.click(screen.getByRole('switch', { name: 'Druckeranbindung' }));
    expect(await screen.findByText('Das hat nicht geklappt: offline')).toBeInTheDocument();
  });

  it('shows the background loading error from the printer link', () => {
    const l = link({ error: 'Netzwerk kaputt' });
    render(<LanguageProvider><PrinterLinkSettings link={l} printers={printers} /></LanguageProvider>);
    expect(screen.getByText('Das hat nicht geklappt: Netzwerk kaputt')).toBeInTheDocument();
  });

  it('does not list resin printers, they have no printer connection', () => {
    const l = link({ enabled: true });
    const withResin: Printer[] = [...printers, { id: '9', name: 'Saturn 4', kind: 'resin', units: [] }];
    render(<LanguageProvider><PrinterLinkSettings link={l} printers={withResin} /></LanguageProvider>);
    expect(screen.getByText('Sovol SV08')).toBeInTheDocument();
    expect(screen.queryByText('Saturn 4')).toBeNull();
  });
});
