import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { useState } from 'react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrintDataRereadProvider, usePrintDataReread } from '../hooks/usePrintDataReread';
import { PrintDataRereadSettings } from './PrintDataRereadSettings';
import { getSession } from '../lib/webSession';
import { de } from '../i18n/de';
import type { ModelFile } from '../types';

let writable = true;
let calls: { command: string; args: Record<string, unknown> }[] = [];
const sliced = {
  id: 'a', name: 'a', path: '/m/a.3mf', weightSource: 'slicer', deletedAt: null, printInfo: {
    sliceState: 'sliced', settingsState: 'read', readerCurrent: true, readAt: 'x', printerModel: 'P', bedType: 'B', layerHeightMm: 0.2,
    nozzleMm: { value: 0.4, mixed: false }, bedTempC: { value: 55, mixed: false }, nozzleTempC: { value: 220, mixed: false }, plates: [], filamentColors: [] },
  materials: [], tags: [],
};
const fetchMock = vi.fn(async (url: string, options?: RequestInit): Promise<Response> => {
  if (!url.startsWith('/api/invoke/')) return Response.json({ authenticated: true, writable, csrfToken: 'token', mustChangePassword: false });
  const command = url.split('/').pop()!;
  calls.push({ command, args: JSON.parse((options?.body as string) || '{}') });
  return Response.json(sliced);
});

const unread = { id: 'a', name: 'a', path: '/m/a.3mf', weightSource: 'slicer', deletedAt: null, printInfo: null, printDataState: 'unread', summaryOnly: true, materials: [], tags: [] } as unknown as ModelFile;
let api: ReturnType<typeof usePrintDataReread>;
function Probe() { api = usePrintDataReread(); return null; }
function Harness() {
  const [models, setModels] = useState([unread]);
  return <LanguageProvider><PrintDataRereadProvider models={models} setModels={setModels}><Probe /><PrintDataRereadSettings /></PrintDataRereadProvider></LanguageProvider>;
}

beforeEach(async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  writable = true; calls = [];
  vi.stubGlobal('fetch', fetchMock);
  await getSession();
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); fetchMock.mockClear(); });

it('web: reads without the desktop-only lean argument', async () => {
  render(<Harness />);
  fireEvent.click(screen.getByRole('button', { name: de.printDataRereadAll }));
  expect(await screen.findByText('1 Datei gelesen.')).toBeInTheDocument();
  expect(calls).toEqual([{ command: 'rescan_file_metadata', args: { fileId: 'a' } }]);
});

it('read-only web: the start button is disabled with the reason and a start does nothing', async () => {
  writable = false;
  await getSession();
  render(<Harness />);
  const button = screen.getByRole('button', { name: de.printDataRereadAll });
  await waitFor(() => expect(button).toBeDisabled());
  expect(button).toHaveAttribute('title', de.webReadOnly);
  expect(button).toHaveAccessibleDescription(de.webReadOnly);
  expect(api.start('settings')).toBe('readOnly');
  expect(api.start('selection', ['a'])).toBe('readOnly');
  expect(calls).toEqual([]);
});
