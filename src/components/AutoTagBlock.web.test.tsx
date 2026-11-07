import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { DetailPanel } from './DetailPanel';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import { getSession } from '../lib/webSession';
import { de } from '../i18n/de';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => ({}) }));

const model = makeModelFile({ tags: ['miniatur', 'mein-projekt'] });
const noop = () => {};
let writable = true;
let calls: { command: string; args: Record<string, unknown> }[] = [];

beforeEach(async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.setItem('3mf-katalog-density', 'compact');
  writable = true; calls = [];
  vi.stubGlobal('fetch', vi.fn(async (url: string, options?: RequestInit) => {
    if (!url.startsWith('/api/invoke/')) return Response.json({ authenticated: true, writable, csrfToken: 'token', mustChangePassword: false });
    const command = url.split('/').pop()!;
    calls.push({ command, args: JSON.parse((options?.body as string) || '{}') });
    if (command === 'list_suggested_auto_tags') return Response.json(['miniatur']);
    return Response.json(null);
  }));
  await getSession();
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

const renderPanel = (onRemoveTag = vi.fn()) => {
  render(<LanguageProvider><UiDensityProvider><DetailPanel model={model} allTags={[]}
    onAddTag={vi.fn()} onRemoveTag={onRemoveTag} onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop}
    onToggleQueue={noop} onUploadImage={noop} onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop}
    slicerError={null} /></UiDensityProvider></LanguageProvider>);
  return onRemoveTag;
};

it('offers remove-and-block in the web build and sends the block through the web command', async () => {
  let onRemoveTag!: ReturnType<typeof renderPanel>;
  // Complete the async suggestions render (including passive effects) before the click.
  await act(async () => { onRemoveTag = renderPanel(); });
  fireEvent.click(await screen.findByRole('button', { name: 'miniatur entfernen, Menü' }, { timeout: 5000 }));
  fireEvent.click(await screen.findByRole('menuitem', { name: de.autoTagRemoveAndBlock }, { timeout: 5000 }));
  await waitFor(() => expect(calls.find(c => c.command === 'block_auto_tag')?.args).toEqual({ tag: 'miniatur' }));
  expect(calls.find(c => c.command === 'list_suggested_auto_tags')?.args).toEqual({ fileId: 'model-1' });
  expect(onRemoveTag).toHaveBeenCalledWith('miniatur');
});

it('shows no remove control and requests no suggestions for a read-only session', async () => {
  writable = false;
  await getSession();
  renderPanel();
  await screen.findByText(de.hashtagsHeading);
  expect(screen.queryByRole('button', { name: /entfernen/ })).toBeNull();
  expect(calls.map(c => c.command)).not.toContain('list_suggested_auto_tags');
  expect(calls.map(c => c.command)).not.toContain('block_auto_tag');
});

it('keeps the tag menu usable after delayed suggestions and a session refresh', async () => {
  const originalFetch = fetch;
  let release!: (response: Response) => void;
  vi.stubGlobal('fetch', vi.fn((url: string, options?: RequestInit) => url.endsWith('/list_suggested_auto_tags')
    ? new Promise<Response>(resolve => { release = resolve; }) : originalFetch(url, options)));
  const errors = vi.spyOn(console, 'error');
  try {
    await act(async () => { renderPanel(); });
    expect(screen.getByRole('button', { name: 'miniatur entfernen' })).toBeVisible();
    expect(screen.queryByRole('button', { name: 'miniatur entfernen, Menü' })).toBeNull();
    await act(async () => { await getSession(); release(Response.json(['miniatur'])); });
    const button = screen.getByRole('button', { name: 'miniatur entfernen, Menü' });
    fireEvent.click(button);
    expect(button).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('menuitem', { name: de.autoTagRemoveAndBlock })).toBeVisible();
    expect(errors.mock.calls.flat().join(' ')).not.toContain('same key');
  } finally { errors.mockRestore(); }
});
