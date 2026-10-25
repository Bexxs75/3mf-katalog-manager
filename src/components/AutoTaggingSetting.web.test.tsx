import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { LanguageProvider } from '../i18n/LanguageContext';
import { AutoTaggingSetting } from './AutoTaggingSetting';
import { getSession } from '../lib/webSession';
import { de } from '../i18n/de';

let writable = true;
let stored = true;
let calls: { command: string; args: Record<string, unknown> }[] = [];
const fetchMock = vi.fn(async (url: string, options?: RequestInit): Promise<Response> => {
  if (!url.startsWith('/api/invoke/')) return Response.json({ authenticated: true, writable, csrfToken: 'token', mustChangePassword: false });
  const command = url.split('/').pop()!;
  const args = JSON.parse((options?.body as string) || '{}');
  calls.push({ command, args });
  if (command === 'set_auto_tagging') { stored = args.enabled; return Response.json(null); }
  if (command === 'get_auto_tagging') return Response.json(stored);
  return Response.json({ message: 'unexpected command', expected: false }, { status: 500 });
});

beforeEach(async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  writable = true; stored = true; calls = [];
  vi.stubGlobal('fetch', fetchMock);
  await getSession();
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); fetchMock.mockClear(); });

const setup = () => render(<LanguageProvider><AutoTaggingSetting /></LanguageProvider>);

it('reads and writes the switch through the web commands and shows no block list', async () => {
  stored = false;
  setup();
  const box = await screen.findByRole('switch', { name: de.autoTaggingLabel });
  await waitFor(() => expect(box).toHaveAttribute('aria-checked', 'false'));
  fireEvent.click(box);
  await waitFor(() => expect(box).toHaveAttribute('aria-checked', 'true'));
  expect(calls.map(c => c.command)).toEqual(['get_auto_tagging', 'set_auto_tagging']);
  expect(calls[1].args).toEqual({ enabled: true });
  expect(screen.queryByText(de.autoTaggingBlockedNote)).toBeNull();
});

it('disables the switch for a read-only session', async () => {
  writable = false;
  await getSession();
  setup();
  const box = await screen.findByRole('switch', { name: de.autoTaggingLabel });
  await waitFor(() => expect(calls.map(c => c.command)).toEqual(['get_auto_tagging']));
  expect(box).toBeDisabled();
});
