import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
vi.mock('../lib/transport', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
import { LanguageProviderWithDiagnostics } from '../test/renderWithDiagnostics';
import { login, authenticationFailed } from '../lib/webSession';
import { PrinterHistoryTable } from './PrinterHistoryTable';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { PrinterHistoryJob } from '../types';

beforeEach(() => { vi.stubEnv('VITE_TARGET', 'web'); localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
afterEach(() => { cleanup(); authenticationFailed(401); vi.unstubAllGlobals(); vi.clearAllMocks(); vi.unstubAllEnvs(); });

const jobs = [{
  id: '1', fileName: 'clog.gcode', outcome: 'partial', printDurationS: 60, usedMm: 40, grams: null, state: 'open', endedAt: 1000,
  rawStatus: 'cancelled', failureReason: 'clog', failureNote: null, autoClosed: false,
}] as PrinterHistoryJob[];
const show = () => render(<LanguageProviderWithDiagnostics>
  <PrinterHistoryTable printerId="1" jobs={jobs} link={{ jobs: [] } as unknown as PrinterLinkState} onMaterial={vi.fn()} onChanged={vi.fn()} />
</LanguageProviderWithDiagnostics>);

it('lets a session that may edit set the reason and review open jobs in the web version', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ authenticated: true, writable: true, csrfToken: 'csrf' })));
  await login('test');
  show();
  expect(screen.getByText('Düse verstopft / Unterextrusion')).toBeInTheDocument();
  expect(screen.getByRole('button', { name: /Grund bearbeiten/ })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Im Material Manager bestätigen' })).toBeInTheDocument();
  expect(screen.queryByText(/Dieser Server zeigt den Grund nur an/)).toBeNull();
});

it('keeps the reason read-only for a session that may not edit', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ authenticated: true, writable: false, csrfToken: 'csrf' })));
  await login('test');
  show();
  expect(screen.queryByRole('button', { name: /Grund bearbeiten/ })).toBeNull();
});
