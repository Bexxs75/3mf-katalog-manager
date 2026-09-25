import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrinterLinkSettings } from './PrinterLinkSettings';
import type { PrinterLinkState } from '../hooks/usePrinterLink';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function link(over: Partial<PrinterLinkState> = {}): PrinterLinkState {
  return {
    enabled: false, connections: [], jobs: [], error: null,
    refresh: vi.fn(), setEnabled: vi.fn().mockResolvedValue(undefined), testConnection: vi.fn(),
    removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(),
    ...over,
  } as PrinterLinkState;
}

const printers = [
  { id: '1', name: 'Sovol SV08', units: [] },
  { id: '2', name: 'Werkstatt-Drucker', units: [] },
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
        connectedSince: 1, lastSyncedAt: 2, lastError: null, errorSince: null, paused: false }],
    });
    render(<LanguageProvider><PrinterLinkSettings link={l} printers={printers} /></LanguageProvider>);
    expect(screen.getByText('Klipper · verbunden')).toBeInTheDocument();
    expect(screen.getByText('nicht angebunden')).toBeInTheDocument();
  });
});
