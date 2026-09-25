import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrinterConnectionSection } from './PrinterConnectionSection';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { PrinterConnection } from '../types';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function link(testResult: unknown): PrinterLinkState {
  return {
    enabled: true, connections: [], jobs: [], error: null, refresh: vi.fn(), setEnabled: vi.fn(),
    testConnection: vi.fn().mockResolvedValue(testResult), removeConnection: vi.fn().mockResolvedValue(undefined),
    syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(),
  } as unknown as PrinterLinkState;
}

const okConnection: PrinterConnection = {
  printerId: '1', kind: 'moonraker', address: '192.168.1.60', baseUrl: 'http://192.168.1.60',
  remoteVersion: 'v0.8.0-209', connectedSince: 1790271120, lastSyncedAt: null, lastError: null, errorSince: null, paused: false,
};

function renderIt(l: PrinterLinkState, connection: PrinterConnection | null = null) {
  render(<LanguageProvider><PrinterConnectionSection printerId="1" connection={connection} link={l} /></LanguageProvider>);
}

describe('PrinterConnectionSection', () => {
  it('tests the typed address and shows the success', async () => {
    const l = link({ ok: true, error: null, connection: okConnection });
    renderIt(l);
    fireEvent.change(screen.getByLabelText('Adresse (IP oder Name)'), { target: { value: '192.168.1.60' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung testen' })));
    expect(l.testConnection).toHaveBeenCalledWith('1', '192.168.1.60');
    expect(screen.getByText('Verbunden')).toBeInTheDocument();
    expect(screen.getByText(/v0\.8\.0-209 · Port 80/)).toBeInTheDocument();
  });

  it.each([
    ['unreachable', /Nicht erreichbar/],
    ['auth_required', /Anmeldung nötig/],
    ['address_not_allowed', /Nur Adressen im Heimnetz/],
    ['history_missing', /Druckhistorie ist am Drucker nicht eingeschaltet/],
    ['disabled', /Die Druckeranbindung ist ausgeschaltet/],
  ])('explains the error %s', async (code, text) => {
    renderIt(link({ ok: false, error: code, connection: null }));
    fireEvent.change(screen.getByLabelText('Adresse (IP oder Name)'), { target: { value: '10.0.0.9' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung testen' })));
    expect(screen.getByText(text)).toBeInTheDocument();
  });

  it('removes an existing connection', async () => {
    const l = link(null);
    renderIt(l, okConnection);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung entfernen' })));
    expect(l.removeConnection).toHaveBeenCalledWith('1');
  });

  it('shows an error and does not throw when testConnection rejects', async () => {
    const l = link(undefined);
    l.testConnection = vi.fn().mockRejectedValue(new Error('lock poisoned'));
    renderIt(l);
    fireEvent.change(screen.getByLabelText('Adresse (IP oder Name)'), { target: { value: '10.0.0.9' } });
    await expect(
      act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung testen' }))),
    ).resolves.not.toThrow();
    expect(screen.getByText('Das hat nicht geklappt: lock poisoned')).toBeInTheDocument();
  });

  it('shows an error and does not throw when removeConnection rejects', async () => {
    const l = link(null);
    l.removeConnection = vi.fn().mockRejectedValue(new Error('unknown printer id'));
    renderIt(l, okConnection);
    const removeButton = screen.getByRole('button', { name: 'Verbindung entfernen' });
    await expect(act(async () => fireEvent.click(removeButton))).resolves.not.toThrow();
    expect(screen.getByText('Das hat nicht geklappt: unknown printer id')).toBeInTheDocument();
    expect(removeButton).not.toBeDisabled();
  });
});
