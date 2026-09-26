import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { clockOffsetParts, PrinterConnectionSection } from './PrinterConnectionSection';
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
  remoteVersion: 'v0.8.0-209', connectedSince: 1790271120, lastSyncedAt: null, lastError: null, errorSince: null, paused: false, clockOffsetS: 0,
};

function renderIt(l: PrinterLinkState, connection: PrinterConnection | null = null) {
  return render(<LanguageProvider><PrinterConnectionSection printerId="1" connection={connection} link={l} /></LanguageProvider>);
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

  it('does not claim "Verbunden" while the connection is paused (e.g. after a backup restore)', () => {
    // After a restore `paused = 1` is set without `lastError`; the section
    // must not show "Connected" then.
    renderIt(link(null), { ...okConnection, paused: true, lastError: null });
    expect(screen.queryByText('Verbunden')).not.toBeInTheDocument();
  });

  it('does not claim "Verbunden" while a lastError is set, even if paused is false', () => {
    renderIt(link(null), { ...okConnection, paused: false, lastError: 'unreachable', errorSince: 1 });
    expect(screen.queryByText('Verbunden')).not.toBeInTheDocument();
  });

  it('reflects a later connection prop update instead of only the one seen on mount', () => {
    const l = link(null);
    const { rerender } = renderIt(l, okConnection);
    expect(screen.getByText('Verbunden')).toBeInTheDocument();
    // An updated `connection` from outside must arrive.
    const paused = { ...okConnection, paused: true, lastError: null };
    rerender(
      <LanguageProvider>
        <PrinterConnectionSection printerId="1" connection={paused} link={l} />
      </LanguageProvider>,
    );
    expect(screen.queryByText('Verbunden')).not.toBeInTheDocument();
  });

  it('does not let a stale prop update clobber a just-returned test result', async () => {
    // A later, older `connection` prop must not discard the fresh test
    // result.
    const freshConnection: PrinterConnection = { ...okConnection, remoteVersion: 'v0.9.0-fresh' };
    const l = link({ ok: true, error: null, connection: freshConnection });
    const { rerender } = renderIt(l, null);
    fireEvent.change(screen.getByLabelText('Adresse (IP oder Name)'), { target: { value: '192.168.1.60' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung testen' })));
    expect(screen.getByText(/v0\.9\.0-fresh/)).toBeInTheDocument();
    const staleConnection: PrinterConnection = { ...okConnection, remoteVersion: null };
    rerender(
      <LanguageProvider>
        <PrinterConnectionSection printerId="1" connection={staleConnection} link={l} />
      </LanguageProvider>,
    );
    expect(screen.getByText(/v0\.9\.0-fresh/)).toBeInTheDocument();
  });

  it('explains a wrong printer clock under "Connected"', () => {
    // Qidi Smart 3 from the test report: clock in December 2023.
    const offset = -(2 * 365.25 + 9 * 30.4375 + 15) * 86400;
    renderIt(link(null), { ...okConnection, clockOffsetS: offset });
    expect(screen.getByText('Die Uhr des Druckers geht falsch')).toBeInTheDocument();
    expect(screen.getByText(/Abweichung von etwa 2 Jahren und 9 Monaten aus/)).toBeInTheDocument();
    expect(screen.getByText(/^Drucker: .* · Computer: /)).toBeInTheDocument();
  });

  it('shows no clock note for a correct clock or a broken connection', () => {
    const { unmount } = renderIt(link(null), okConnection);
    expect(screen.queryByText('Die Uhr des Druckers geht falsch')).not.toBeInTheDocument();
    unmount();
    renderIt(link(null), { ...okConnection, clockOffsetS: -1e8, lastError: 'unreachable', errorSince: 1 });
    expect(screen.queryByText('Die Uhr des Druckers geht falsch')).not.toBeInTheDocument();
  });
});

describe('clockOffsetParts', () => {
  it('picks a rough unit', () => {
    expect(clockOffsetParts(600)).toEqual([['minutes', 10]]);
    expect(clockOffsetParts(-3 * 3600)).toEqual([['hours', 3]]);
    expect(clockOffsetParts(5 * 86400)).toEqual([['days', 5]]);
    expect(clockOffsetParts(-(365.25 * 86400 + 10 * 86400))).toEqual([['years', 1]]);
    expect(clockOffsetParts(90 * 86400)).toEqual([['months', 2]]);
  });
});
