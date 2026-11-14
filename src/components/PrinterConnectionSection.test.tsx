import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { clockOffsetParts, PrinterConnectionSection } from './PrinterConnectionSection';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { PrinterConnection } from '../types';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function link(testResult: unknown): PrinterLinkState {
  return {
    enabled: true, connections: [], jobs: [], error: null, refresh: vi.fn(), setEnabled: vi.fn(),
    testConnection: vi.fn().mockResolvedValue(testResult), removeConnection: vi.fn().mockResolvedValue(undefined),
    clearApiKey: vi.fn().mockResolvedValue(undefined),
    includeOlderHistory: vi.fn().mockResolvedValue({ added: 0, error: null }),
    syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(),
  } as unknown as PrinterLinkState;
}

const okConnection: PrinterConnection = {
  printerId: '1', kind: 'moonraker', address: '192.168.1.60', baseUrl: 'http://192.168.1.60',
  remoteVersion: 'v0.8.0-209', connectedSince: 1790271120, lastSyncedAt: null, lastError: null, errorSince: null, paused: false, clockOffsetS: 0,
};

/** The test button in every language (the key field has a second button, so a bare role query is ambiguous). */
const connectButton = /^(Verbindung (testen|einrichten)|Test connection|Set up connection|Probar conexión|Configurar conexión|Tester la connexion|Configurer la connexion)$/;

function renderIt(l: PrinterLinkState, connection: PrinterConnection | null = null) {
  return render(<LanguageProvider><PrinterConnectionSection printerId="1" connection={connection} link={l} /></LanguageProvider>);
}

describe('PrinterConnectionSection', () => {
  it('tests the typed address and shows the success', async () => {
    const l = link({ ok: true, error: null, connection: okConnection });
    renderIt(l);
    fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '192.168.1.60' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung (testen|einrichten)/ })));
    expect(l.testConnection).toHaveBeenCalledWith('1', '192.168.1.60', null);
    expect(screen.getByText('Verbunden')).toBeInTheDocument();
    expect(screen.getByText(/v0\.8\.0-209 · Port 80/)).toBeInTheDocument();
  });

  it.each([
    ['unreachable', /Nicht erreichbar/],
    ['timeout', /antwortet zu langsam/],
    ['connection_refused', /lehnt die Verbindung ab\. Läuft Moonraker/],
    ['connection_reset', /Verbindung zum Drucker wurde abgebrochen/],
    ['server_busy', /Moonraker startet gerade neu/],
    ['name_resolution_failed', /Der Name „10.0.0.9“ konnte nicht aufgelöst werden/],
    ['container_local_name', /\*\.local.*IP/],
    ['container_loopback', /Nur Adressen im Heimnetz.*host.docker.internal:host-gateway/],
    ['auth_required', /Anmeldung nötig.*Feld „API-Schlüssel“/],
    ['auth_rejected', /API-Schlüssel wurde abgelehnt/],
    ['api_key_invalid', /keine Leerzeichen, Umlaute/],
    ['address_not_allowed', /Nur Adressen im Heimnetz/],
    ['history_missing', /Druckhistorie ist am Drucker nicht eingeschaltet/],
    ['disabled', /Die Druckeranbindung ist ausgeschaltet/],
  ])('explains the error %s', async (code, text) => {
    renderIt(link({ ok: false, error: code, connection: null }));
    fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '10.0.0.9' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung (testen|einrichten)/ })));
    expect(screen.getByText(text)).toBeInTheDocument();
  });

  it('shows a printer that is only restarting as a quiet status, not as an alarm', () => {
    const c: PrinterConnection = { ...okConnection, lastError: 'server_busy', errorSince: 1790271000, failures: 1, status: 'reconnecting' };
    renderIt(link(null), c);
    expect(screen.queryByRole('alert')).toBeNull();
    const box = screen.getByRole('status');
    expect(box).toHaveTextContent('Verbinde neu');
    expect(box).toHaveTextContent('Versuch 2');
    expect(box).toHaveTextContent('Moonraker startet gerade neu');
  });

  it('shows a switched-off printer as grey "not reachable since", still without an alarm', () => {
    const c: PrinterConnection = { ...okConnection, lastError: 'connection_refused', errorSince: 1790271000, failures: 4, status: 'offline' };
    renderIt(link(null), c);
    expect(screen.queryByRole('alert')).toBeNull();
    const box = screen.getByRole('status');
    expect(box).toHaveTextContent('Nicht erreichbar');
    expect(box).toHaveTextContent(/nicht erreichbar seit/);
    expect(box).toHaveTextContent('Läuft Moonraker?');
  });

  it('keeps the red alarm for an error that needs the user and for a printer gone for a day', () => {
    for (const lastError of ['auth_required', 'auth_rejected', 'timeout'] as const) {
      const c: PrinterConnection = { ...okConnection, lastError, errorSince: 1, failures: 9, status: 'error' };
      const { unmount } = renderIt(link(null), c);
      expect(screen.getByRole('alert')).toBeInTheDocument();
      unmount();
    }
  });

  it('shows a failed test the user started as an alarm even if the stored status is quiet', async () => {
    const c: PrinterConnection = { ...okConnection, lastError: 'timeout', errorSince: 1, failures: 1, status: 'reconnecting' };
    renderIt(link({ ok: false, error: 'connection_refused', connection: null }), c);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung testen/ })));
    expect(screen.getByRole('alert')).toHaveTextContent('Läuft Moonraker?');
  });

  describe('API key', () => {
    const keyField = () => screen.getByLabelText('API-Schlüssel') as HTMLInputElement;

    it('is a hidden field that can be shown, with the promise about backups next to it', () => {
      renderIt(link(null));
      expect(keyField().type).toBe('password');
      expect(keyField().autocomplete).toBe('off');
      expect(keyField()).toHaveAttribute('placeholder', 'Nur nötig, wenn dein Drucker eine Anmeldung verlangt');
      expect(screen.getByText('Der Schlüssel liegt nur auf diesem Rechner und wird nicht in Sicherungen gespeichert.')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Anzeigen' }));
      expect(keyField().type).toBe('text');
      fireEvent.click(screen.getByRole('button', { name: 'Verbergen' }));
      expect(keyField().type).toBe('password');
    });

    it('sends what was typed with the test and empties the field once the key is saved', async () => {
      const saved: PrinterConnection = { ...okConnection, hasApiKey: true };
      const l = link({ ok: true, error: null, connection: saved });
      renderIt(l);
      fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '192.168.1.60' } });
      fireEvent.change(keyField(), { target: { value: '  0123456789abcdef  ' } });
      await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung (testen|einrichten)/ })));
      expect(l.testConnection).toHaveBeenCalledWith('1', '192.168.1.60', '0123456789abcdef');
      expect(screen.getByText('Schlüssel gespeichert')).toBeInTheDocument();
      expect(screen.queryByLabelText('API-Schlüssel')).toBeNull();
      expect(document.body.innerHTML).not.toContain('0123456789abcdef');
    });

    it('never puts a stored key into the field: it shows only that one exists', () => {
      renderIt(link(null), { ...okConnection, hasApiKey: true });
      expect(screen.getByText('Schlüssel gespeichert')).toBeInTheDocument();
      expect(screen.queryByLabelText('API-Schlüssel')).toBeNull();
      expect(document.querySelectorAll('input[type=password]')).toHaveLength(0);
    });

    it('tests with the stored key when nothing is typed', async () => {
      const l = link({ ok: true, error: null, connection: { ...okConnection, hasApiKey: true } });
      renderIt(l, { ...okConnection, hasApiKey: true });
      await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung testen/ })));
      expect(l.testConnection).toHaveBeenCalledWith('1', '192.168.1.60', null);
    });

    it('replaces a stored key through an empty field', async () => {
      const l = link({ ok: true, error: null, connection: { ...okConnection, hasApiKey: true } });
      renderIt(l, { ...okConnection, hasApiKey: true });
      fireEvent.click(screen.getByRole('button', { name: 'Ersetzen' }));
      expect(keyField().value).toBe('');
      fireEvent.change(keyField(), { target: { value: 'ffffffffffffffff' } });
      await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung testen/ })));
      expect(l.testConnection).toHaveBeenCalledWith('1', '192.168.1.60', 'ffffffffffffffff');
    });

    it('removes a stored key and offers the empty field again', async () => {
      const l = link(null);
      renderIt(l, { ...okConnection, hasApiKey: true });
      await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Entfernen' })));
      expect(l.clearApiKey).toHaveBeenCalledWith('1');
      expect(keyField().value).toBe('');
    });

    it('keeps the typed key in the field when the test fails, so the user can correct it', async () => {
      const l = link({ ok: false, error: 'auth_rejected', connection: null });
      renderIt(l);
      fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '192.168.1.60' } });
      fireEvent.change(keyField(), { target: { value: 'wrongwrongwrong' } });
      await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung (testen|einrichten)/ })));
      expect(screen.getByRole('alert')).toHaveTextContent('API-Schlüssel wurde abgelehnt');
      expect(keyField().value).toBe('wrongwrongwrong');
    });
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
    fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '10.0.0.9' } });
    await expect(
      act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung (testen|einrichten)/ }))),
    ).resolves.not.toThrow();
    expect(screen.getByRole('alert')).toHaveTextContent('Das hat nicht geklappt: lock poisoned');
  });

  it('shows an error and does not throw when removeConnection rejects', async () => {
    const l = link(null);
    l.removeConnection = vi.fn().mockRejectedValue(new Error('unknown printer id'));
    renderIt(l, okConnection);
    const removeButton = screen.getByRole('button', { name: 'Verbindung entfernen' });
    await expect(act(async () => fireEvent.click(removeButton))).resolves.not.toThrow();
    expect(screen.getByRole('alert')).toHaveTextContent('Das hat nicht geklappt: unknown printer id');
    expect(removeButton).not.toBeDisabled();
  });

  it('offers "Report problem" for an unexpected command rejection', async () => {
    const l = link(undefined);
    l.testConnection = vi.fn().mockRejectedValue({ message: 'x', expected: false });
    renderIt(l);
    fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '10.0.0.9' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung (testen|einrichten)/ })));
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });

  it('does not offer "Report problem" for an expected command rejection', async () => {
    const l = link(undefined);
    l.testConnection = vi.fn().mockRejectedValue({ message: 'x', expected: true });
    renderIt(l);
    fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '10.0.0.9' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung (testen|einrichten)/ })));
    expect(screen.getByRole('alert')).toHaveTextContent('x');
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
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
    fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '192.168.1.60' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: /Verbindung (testen|einrichten)/ })));
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

it('keeps the saved connection removable while global sync is off', async () => {
  const l = { ...link(null), enabled: false };
  renderIt(l, okConnection);
  expect(screen.queryByText('Verbunden')).toBeNull();
  expect(screen.getByText(/Schalte unten in der Druckerliste/)).toBeInTheDocument();
  expect(screen.getByLabelText('Hostname oder IP-Adresse')).toHaveValue(okConnection.address);
  expect(screen.getByRole('button', { name: 'Verbindung testen' })).toBeDisabled();
  expect(screen.getByLabelText('API-Schlüssel')).toBeDisabled();
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung entfernen' })));
  expect(l.removeConnection).toHaveBeenCalledWith('1');
  expect(l.testConnection).not.toHaveBeenCalled();
});

it.each([
  ['de', /konnte nicht aufgelöst/, /Namen funktionieren im Container nicht/, /Container selbst/],
  ['en', /could not be resolved/, /names do not work in the container/, /container itself/],
  ['es', /No se pudo resolver/, /no funcionan en el contenedor/, /propio contenedor/],
  ['fr', /n’a pas pu être résolu/, /ne fonctionnent pas dans le conteneur/, /conteneur lui-même/],
])('renders translated printer guidance in %s', async (language, dns, local, loopback) => {
  localStorage.setItem('3mf-katalog-language', language as string);
  for (const [code, pattern] of [
    ['name_resolution_failed', dns], ['container_local_name', local], ['container_loopback', loopback],
  ] as const) {
    const view = renderIt(link({ ok: false, error: code, connection: null }));
    fireEvent.change(screen.getByPlaceholderText('192.168.1.60'), { target: { value: 'printer.invalid:7125' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: connectButton })));
    expect(screen.getByRole('alert')).toHaveTextContent(pattern as RegExp);
    if (code === 'name_resolution_failed') {
      expect(screen.getByRole('alert')).toHaveTextContent('printer.invalid');
      expect(screen.getByRole('alert')).not.toHaveTextContent(':7125');
      expect(screen.getByRole('alert')).not.toHaveTextContent('{host}');
    }
    view.unmount();
  }
});

it('keeps DNS errors tied to the tested host when the input changes', async () => {
  const l = link(null);
  let finish!: (value: unknown) => void;
  l.testConnection = vi.fn().mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  renderIt(l);
  const input = screen.getByPlaceholderText('192.168.1.60');
  fireEvent.change(input, { target: { value: 'old.invalid:7125' } });
  fireEvent.click(screen.getByRole('button', { name: connectButton }));
  fireEvent.change(input, { target: { value: 'new.invalid' } });
  await act(async () => finish({ ok: false, error: 'name_resolution_failed', connection: null }));
  expect(screen.getByRole('alert')).toHaveTextContent('„old.invalid“');
  expect(screen.getByRole('alert')).not.toHaveTextContent('new.invalid');
});

it('uses the saved host for background DNS errors even while editing', () => {
  renderIt(link(null), { ...okConnection, address: 'saved.invalid:7125', lastError: 'name_resolution_failed' });
  fireEvent.change(screen.getByPlaceholderText('192.168.1.60'), { target: { value: 'edited.invalid' } });
  expect(screen.getByRole('alert')).toHaveTextContent('„saved.invalid“');
});

describe('older history', () => {
  const button = () => screen.getByRole('button', { name: 'Älteren Verlauf einbeziehen' });

  it('is offered only for a connected printer and says that nothing is deducted', () => {
    renderIt(link(null));
    expect(screen.queryByRole('button', { name: 'Älteren Verlauf einbeziehen' })).toBeNull();
    renderIt(link(null), okConnection);
    expect(button()).toBeEnabled();
    expect(screen.getByText(/Es wird nichts abgebucht und nichts zur Bestätigung angeboten/)).toBeInTheDocument();
  });

  it('asks for this printer and reports how many prints were taken over', async () => {
    const l = link(null);
    (l.includeOlderHistory as ReturnType<typeof vi.fn>).mockResolvedValue({ added: 12, error: null });
    renderIt(l, okConnection);
    await act(async () => fireEvent.click(button()));
    expect(l.includeOlderHistory).toHaveBeenCalledWith('1');
    expect(screen.getByRole('status')).toHaveTextContent('12 ältere Drucke als Verlauf übernommen.');
  });

  it('says so when nothing new was found', async () => {
    renderIt(link(null), okConnection);
    await act(async () => fireEvent.click(button()));
    expect(screen.getByRole('status')).toHaveTextContent('Keine weiteren älteren Drucke gefunden.');
  });

  it('explains why the printer could not be read', async () => {
    const l = link(null);
    (l.includeOlderHistory as ReturnType<typeof vi.fn>).mockResolvedValue({ added: 0, error: 'connection_refused' });
    renderIt(l, okConnection);
    await act(async () => fireEvent.click(button()));
    expect(screen.getByRole('alert')).toHaveTextContent(/lehnt die Verbindung ab/);
  });

  it('is off while the connection is switched off', () => {
    renderIt({ ...link(null), enabled: false } as PrinterLinkState, okConnection);
    expect(button()).toBeDisabled();
  });
});
