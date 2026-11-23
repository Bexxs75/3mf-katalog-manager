import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));

const api = vi.hoisted(() => ({
  getPrinterLinkEnabled: vi.fn(), listPrinterConnections: vi.fn(), listOpenPrinterJobs: vi.fn(),
  PRINTER_JOBS_CHANGED_EVENT: 'printer-jobs-changed',
}));
vi.mock('../lib/api/printerLink', () => api);
const listeners = vi.hoisted(() => ({} as Record<string, () => void>));
vi.mock('../lib/events', () => ({
  listen: vi.fn((name: string, cb: () => void) => { listeners[name] = cb; return Promise.resolve(() => undefined); }),
}));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { usePrinterLink, type PrinterLinkState } from '../hooks/usePrinterLink';
import { PrinterConnectionSection } from './PrinterConnectionSection';
import { PrinterLinkControl } from './PrinterLinkControl';
import { PrinterLinkStatus } from './PrinterLinkStatus';
import type { PrinterConnection } from '../types';

beforeEach(() => {
  vi.stubEnv('VITE_TARGET', 'web');
  localStorage.setItem('3mf-katalog-language', 'de');
  api.getPrinterLinkEnabled.mockResolvedValue(true);
  api.listPrinterConnections.mockResolvedValue([]);
  api.listOpenPrinterJobs.mockResolvedValue([]);
});
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.unstubAllEnvs(); });

const connection: PrinterConnection = {
  printerId: '1', kind: 'moonraker', address: '192.168.1.60', baseUrl: 'http://192.168.1.60',
  remoteVersion: 'v0.8.0', connectedSince: 1790271120, lastSyncedAt: null, lastError: null, errorSince: null,
  paused: false, clockOffsetS: 0, hasApiKey: true,
};
function link(over: Partial<PrinterLinkState> = {}): PrinterLinkState {
  return {
    enabled: true, unavailable: false, connections: [connection], jobs: [], error: null, refresh: vi.fn(),
    setEnabled: vi.fn().mockResolvedValue(undefined), testConnection: vi.fn(), clearApiKey: vi.fn().mockResolvedValue(undefined),
    removeConnection: vi.fn(), syncNow: vi.fn().mockResolvedValue(undefined), includeOlderHistory: vi.fn(),
    ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(), ...over,
  } as unknown as PrinterLinkState;
}
const wrap = (ui: React.ReactElement) => render(<LanguageProvider>{ui}</LanguageProvider>);

it('loads the connection state in the web version and reloads on a server event', async () => {
  const { result } = renderHook(() => usePrinterLink());
  await waitFor(() => expect(result.current.enabled).toBe(true));
  expect(api.listOpenPrinterJobs).toHaveBeenCalledTimes(1);
  await waitFor(() => expect(listeners['printer-jobs-changed']).toBeDefined());
  await act(async () => { listeners['printer-jobs-changed'](); await Promise.resolve(); });
  expect(api.listOpenPrinterJobs).toHaveBeenCalledTimes(2);
});

it('treats a server without printer allowlist as unavailable, not as a failure', async () => {
  api.getPrinterLinkEnabled.mockRejectedValue({ message: 'printerAddressNotAllowed', expected: true });
  const { result } = renderHook(() => usePrinterLink());
  await waitFor(() => expect(result.current.unavailable).toBe(true));
  expect(result.current.error).toBeNull();
});

it('still reports other server errors, e.g. maintenance', async () => {
  api.getPrinterLinkEnabled.mockRejectedValue({ message: 'maintenance', expected: true });
  const { result } = renderHook(() => usePrinterLink());
  await waitFor(() => expect(result.current.error?.message).toBe('maintenance'));
  expect(result.current.unavailable).toBe(false);
});

it('explains who connects to the printer and never offers to reveal a stored key', () => {
  wrap(<PrinterConnectionSection printerId="1" connection={connection} link={link()} />);
  expect(screen.getByText(/Der Server verbindet sich mit dem Drucker, nicht dein Browser/)).toBeInTheDocument();
  expect(screen.getByText('Schlüssel gespeichert')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Anzeigen' })).toBeNull();
  expect(screen.getByRole('button', { name: 'Entfernen' })).toBeInTheDocument();
});

it('keeps a typed key out of the DOM text and clears the field after a successful test', async () => {
  const l = link({ testConnection: vi.fn().mockResolvedValue({ ok: true, error: null, connection }) });
  const { container } = wrap(<PrinterConnectionSection printerId="1" connection={null} link={l} />);
  fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '192.168.1.60' } });
  const key = container.querySelector('input[type="password"]') as HTMLInputElement;
  fireEvent.change(key, { target: { value: 'secret-token-123' } });
  expect(container.textContent).not.toContain('secret-token-123');
  expect(screen.queryByRole('button', { name: 'Anzeigen' })).toBeNull();
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung einrichten' })));
  expect(l.testConnection).toHaveBeenCalledWith('1', '192.168.1.60', 'secret-token-123');
  expect(container.innerHTML).not.toContain('secret-token-123');
});

it('names the server allowlist when the server refuses the address', async () => {
  const l = link({ testConnection: vi.fn().mockResolvedValue({ ok: false, error: 'address_not_allowed', connection: null }) });
  wrap(<PrinterConnectionSection printerId="1" connection={null} link={l} />);
  fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '10.9.9.9' } });
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung einrichten' })));
  expect(screen.getByRole('alert')).toHaveTextContent(/Dieser Server lässt die Adresse nicht zu/);
});

it('shows a time-out of the server-side test with the existing printer text', async () => {
  const l = link({ testConnection: vi.fn().mockResolvedValue({ ok: false, error: 'timeout', connection: null }) });
  wrap(<PrinterConnectionSection printerId="1" connection={null} link={l} />);
  fireEvent.change(screen.getByLabelText('Hostname oder IP-Adresse'), { target: { value: '192.168.1.60' } });
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Verbindung einrichten' })));
  expect(screen.getByRole('alert')).toHaveTextContent(/antwortet zu langsam/);
});

it('shows a coded server refusal of a printer action through the error translation', async () => {
  const l = link({ clearApiKey: vi.fn().mockRejectedValue({ message: 'maintenance', expected: true }) });
  wrap(<PrinterConnectionSection printerId="1" connection={connection} link={l} />);
  await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Entfernen' })));
  expect(screen.getByRole('alert')).toHaveTextContent('Der Katalog wird gewartet. Bitte später erneut versuchen.');
});

it('renders the switch in the web version and an explanation when the server has no printers released', () => {
  const { unmount } = wrap(<PrinterLinkControl link={link()} />);
  expect(screen.getByRole('switch', { name: 'Anbindung' })).toBeInTheDocument();
  unmount();
  wrap(<PrinterLinkControl link={link({ unavailable: true })} />);
  expect(screen.queryByRole('switch')).toBeNull();
  expect(screen.getByRole('status')).toHaveTextContent('Druckeranbindung auf diesem Server nicht eingerichtet');
});

it('shows the printer status row with a sync button in the web version', () => {
  wrap(<PrinterLinkStatus printerId="1" link={link()} />);
  expect(screen.getByRole('button', { name: 'Jetzt verbinden' })).toBeInTheDocument();
});
