import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { login, authenticationFailed } from '../lib/webSession';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { FilamentView } from './FilamentView';
import { usePrinters } from '../hooks/usePrinters';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { FilamentSpool } from '../types';

const base: FilamentSpool = { id: '1', kind: 'filament', material: 'PLA', manufacturer: null, color: 'Rot', colorHex: null, location: 'Regal', homeLocation: null, unitId: null, slotIndex: null, diameterMm: 1.75, originalWeightG: 1000, remainingWeightG: 500, price: null, imagePng: null, colorHexes: [] };
const resin: FilamentSpool = { ...base, id: '2', kind: 'resin', material: 'Standard', color: 'Grau', diameterMm: 0, remainingWeightG: 640 };
let calls: { command: string; args: any }[] = [];
let writable = true;
beforeEach(async () => {
  vi.stubEnv('VITE_TARGET', 'web'); localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); calls = []; writable = true;
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.includes('/invoke/')) return Response.json({ authenticated: true, writable, csrfToken: 't' });
    const command = url.split('/').pop()!; calls.push({ command, args: JSON.parse(String(options?.body)) });
    if (command === 'list_filament_spools') return Response.json([base, resin]);
    if (command === 'restock_filament_spool') return Response.json([{ ...base, id: '9', remainingWeightG: 1000 }]);
    if (command === 'consume_resin') return Response.json({ ...resin, remainingWeightG: 600 });
    return Response.json([]);
  }));
  vi.stubGlobal('IntersectionObserver', class { observe() {} disconnect() {} });
  await login('password');
});
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.unstubAllEnvs(); });
const link = { enabled: false, connections: [], jobs: [], error: null, refresh: vi.fn(), setEnabled: vi.fn(), testConnection: vi.fn(), removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn() } as unknown as PrinterLinkState;
function Host() { const printers = usePrinters(); return <FilamentView printerLink={link} printers={printers} />; }
const view = () => render(<LanguageProvider><Host /></LanguageProvider>);

it('restocks a spool in the web build with the writable session', async () => {
  view();
  fireEvent.click((await screen.findAllByRole('button', { name: 'Nachkaufen' }))[0]);
  fireEvent.click(await screen.findByRole('button', { name: '1 Spule anlegen' }));
  await waitFor(() => expect(calls.some(c => c.command === 'restock_filament_spool')).toBe(true));
  expect(calls.find(c => c.command === 'restock_filament_spool')!.args).toMatchObject({ templateId: '1', count: 1 });
});
it('deducts resin in the web build', async () => {
  localStorage.setItem('3mf-katalog-filament-kind', 'resin');
  view();
  fireEvent.click((await screen.findAllByRole('button', { name: 'Verbrauch' }))[0]);
  fireEvent.change(await screen.findByLabelText('Verbraucht (ml)'), { target: { value: '40' } });
  fireEvent.click(screen.getByRole('button', { name: 'Abbuchen' }));
  await waitFor(() => expect(calls.some(c => c.command === 'consume_resin')).toBe(true));
  expect(calls.find(c => c.command === 'consume_resin')!.args).toEqual({ spoolId: '2', amountMl: 40 });
});
it('keeps the buttons disabled and sends nothing without write mode', async () => {
  writable = false; await login('password');
  view();
  const buttons = await screen.findAllByRole('button', { name: 'Nachkaufen' });
  for (const b of buttons) expect(b).toBeDisabled();
  fireEvent.click(buttons[0]);
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(calls.some(c => c.command === 'restock_filament_spool' || c.command === 'consume_resin')).toBe(false);
});
