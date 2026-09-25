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

  it('shows unreachable and auth states', () => {
    const { rerender } = render(
      <LanguageProvider><PrinterLinkStatus printerId="1" link={link({ ...base, lastError: 'unreachable', errorSince: 1790253900 })} /></LanguageProvider>,
    );
    expect(screen.getByText(/nicht erreichbar seit/)).toBeInTheDocument();
    rerender(<LanguageProvider><PrinterLinkStatus printerId="1" link={link({ ...base, lastError: 'auth_required', paused: true })} /></LanguageProvider>);
    expect(screen.getByText('Anmeldung nötig')).toBeInTheDocument();
  });
});
