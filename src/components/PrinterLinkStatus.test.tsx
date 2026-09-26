import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrinterLinkStatus } from './PrinterLinkStatus';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { PrinterConnection } from '../types';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const base: PrinterConnection = {
  printerId: '1', kind: 'moonraker', address: '192.168.1.60', baseUrl: null, remoteVersion: null,
  connectedSince: 1, lastSyncedAt: Date.now() / 1000 - 120, lastError: null, errorSince: null, paused: false,
};

function link(c: PrinterConnection | null, jobs = 0): PrinterLinkState {
  return {
    enabled: true, connections: c ? [c] : [], error: null, refresh: vi.fn(), setEnabled: vi.fn(), testConnection: vi.fn(),
    removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(),
    jobs: Array.from({ length: jobs }, (_, i) => ({ id: String(i), printerId: '1' })) as PrinterLinkState['jobs'],
  } as PrinterLinkState;
}

describe('PrinterLinkStatus', () => {
  it('renders nothing for printers without connection', () => {
    const { container } = render(<LanguageProvider><PrinterLinkStatus printerId="1" link={link(null)} /></LanguageProvider>);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows last sync, pending jobs and a sync button', () => {
    const l = link(base, 3);
    render(<LanguageProvider><PrinterLinkStatus printerId="1" link={l} /></LanguageProvider>);
    expect(screen.getByText(/abgeglichen/)).toBeInTheDocument();
    expect(screen.getByText('3 Drucke zu bestätigen')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Jetzt abgleichen' }));
    expect(l.syncNow).toHaveBeenCalled();
  });

  it('shows a message when syncing fails', async () => {
    // Tauri often rejects commands with a plain string, not with an
    // Error object - exactly this case is to be caught here.
    const l = link(base);
    l.syncNow = vi.fn().mockRejectedValue('offline');
    render(<LanguageProvider><PrinterLinkStatus printerId="1" link={l} /></LanguageProvider>);
    fireEvent.click(screen.getByRole('button', { name: 'Jetzt abgleichen' }));
    expect(await screen.findByText('Das hat nicht geklappt: offline')).toBeInTheDocument();
  });

  it('shows unreachable and auth states', () => {
    const { rerender } = render(
      <LanguageProvider><PrinterLinkStatus printerId="1" link={link({ ...base, lastError: 'unreachable', errorSince: 1790253900 })} /></LanguageProvider>,
    );
    expect(screen.getByText(/nicht erreichbar seit/)).toBeInTheDocument();
    rerender(<LanguageProvider><PrinterLinkStatus printerId="1" link={link({ ...base, lastError: 'auth_required', paused: true })} /></LanguageProvider>);
    expect(screen.getByText('Anmeldung nötig')).toBeInTheDocument();
  });

  it('shows a paused-retest hint instead of a healthy status when paused without an error', () => {
    // sanitize_printer_connections (backup.rs) sets `paused = 1` after a
    // backup restore, but `lastError` stays empty - without this branch
    // the connection looked "healthy" although the sync skips it
    // forever.
    const l = link({ ...base, paused: true, lastError: null });
    const { container } = render(<LanguageProvider><PrinterLinkStatus printerId="1" link={l} /></LanguageProvider>);
    expect(screen.getByText('pausiert – bitte Verbindung neu testen')).toBeInTheDocument();
    expect(container.querySelector('.bg-\\[var\\(--warn\\)\\]')).toBeInTheDocument();
  });

  it('maps a non-unreachable error through the shared error-key texts instead of "unreachable since"', () => {
    const l = link({ ...base, lastError: 'bad_response', errorSince: 1 });
    render(<LanguageProvider><PrinterLinkStatus printerId="1" link={l} /></LanguageProvider>);
    expect(screen.getByText(/Unerwartete Antwort/)).toBeInTheDocument();
    expect(screen.queryByText(/nicht erreichbar seit/)).not.toBeInTheDocument();
  });

  it('shows the plural form for pending jobs', () => {
    render(<LanguageProvider><PrinterLinkStatus printerId="1" link={link(base, 1)} /></LanguageProvider>);
    expect(screen.getByText('1 Druck zu bestätigen')).toBeInTheDocument();
  });
});
