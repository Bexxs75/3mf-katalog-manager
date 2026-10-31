import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { LanguageProvider } from '../i18n/LanguageContext';
import { AutoTaggingSetting } from './AutoTaggingSetting';
import { getSession } from '../lib/webSession';
import { de } from '../i18n/de';

let writable = true;
let stored = true;
let blocked: string[] = [];
let calls: { command: string; args: Record<string, unknown> }[] = [];
const fetchMock = vi.fn(async (url: string, options?: RequestInit): Promise<Response> => {
  if (!url.startsWith('/api/invoke/')) return Response.json({ authenticated: true, writable, csrfToken: 'token', mustChangePassword: false });
  const command = url.split('/').pop()!;
  const args = JSON.parse((options?.body as string) || '{}');
  calls.push({ command, args });
  if (command === 'set_auto_tagging') { stored = args.enabled; return Response.json(null); }
  if (command === 'get_auto_tagging') return Response.json(stored);
  if (command === 'list_blocked_auto_tags') return Response.json(blocked);
  if (command === 'unblock_auto_tag') { blocked = blocked.filter(t => t !== args.tag); return Response.json(null); }
  return Response.json({ message: 'unexpected command', expected: false }, { status: 500 });
});

beforeEach(async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  writable = true; stored = true; blocked = []; calls = [];
  vi.stubGlobal('fetch', fetchMock);
  await getSession();
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); fetchMock.mockClear(); });

const setup = () => render(<LanguageProvider><AutoTaggingSetting /></LanguageProvider>);

it('reads and writes the switch through the web commands', async () => {
  stored = false;
  setup();
  const box = await screen.findByRole('switch', { name: de.autoTaggingLabel });
  await waitFor(() => expect(box).toHaveAttribute('aria-checked', 'false'));
  fireEvent.click(box);
  await waitFor(() => expect(box).toHaveAttribute('aria-checked', 'true'));
  const commands = calls.map(c => c.command);
  expect(commands).toContain('get_auto_tagging');
  expect(commands.filter(c => c === 'set_auto_tagging')).toHaveLength(1);
  expect(calls.find(c => c.command === 'set_auto_tagging')!.args).toEqual({ enabled: true });
  expect(screen.queryByText(de.autoTaggingBlockedNote)).toBeNull();
});

it('disables the switch for a read-only session', async () => {
  writable = false;
  await getSession();
  setup();
  const box = await screen.findByRole('switch', { name: de.autoTaggingLabel });
  await waitFor(() => expect(calls.map(c => c.command)).toContain('get_auto_tagging'));
  expect(box).toBeDisabled();
  expect(calls.map(c => c.command)).not.toContain('set_auto_tagging');
});

it('lists the block list through the web command and lifts a block', async () => {
  blocked = ['miniatur', 'kabelhalter'];
  setup();
  expect(await screen.findByText(de.blockedAutoTagsTitle)).toBeInTheDocument();
  fireEvent.click(await screen.findByRole('button', { name: 'Sperre für #miniatur aufheben' }));
  await waitFor(() => expect(calls.find(c => c.command === 'unblock_auto_tag')?.args).toEqual({ tag: 'miniatur' }));
  await waitFor(() => expect(screen.queryByText('#miniatur')).toBeNull());
  expect(screen.getByText('#kabelhalter')).toBeInTheDocument();
});

it('keeps the unblock buttons disabled and sends nothing for a read-only session', async () => {
  writable = false;
  blocked = ['miniatur'];
  await getSession();
  setup();
  const button = await screen.findByRole('button', { name: 'Sperre für #miniatur aufheben' });
  expect(button).toBeDisabled();
  fireEvent.click(button);
  expect(calls.map(c => c.command)).not.toContain('unblock_auto_tag');
});
