import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DetailPanel } from './DetailPanel';
import { ModelDetailPage } from './ModelDetailPage';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import { de } from '../i18n/de';
import * as api from '../lib/api/autoTags';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => ({}) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));
vi.mock('../lib/api/autoTags');

const model = makeModelFile({ tags: ['miniatur', 'mein-projekt'] });
const noop = () => {};

function renderPanel(density: string, handlers: { onAddTag?: (t: string) => void; onRemoveTag?: (t: string) => void } = {}) {
  localStorage.setItem('3mf-katalog-density', density);
  const onAddTag = handlers.onAddTag ?? vi.fn();
  const onRemoveTag = handlers.onRemoveTag ?? vi.fn();
  render(<LanguageProvider><UiDensityProvider><DetailPanel model={model} allTags={[]}
    onAddTag={onAddTag} onRemoveTag={onRemoveTag} onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop}
    onToggleQueue={noop} onUploadImage={noop} onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop}
    slicerError={null} /></UiDensityProvider></LanguageProvider>);
  return { onAddTag, onRemoveTag };
}

// Generous timeout: the whole suite runs in parallel workers and the panel renders slowly then.
const removeAuto = () => screen.findByRole('button', { name: 'miniatur entfernen, Menü' }, { timeout: 5000 });

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  vi.mocked(api.listSuggestedAutoTags).mockReset().mockResolvedValue(['miniatur', 'kabelhalter']);
  vi.mocked(api.blockAutoTag).mockReset().mockResolvedValue(undefined);
  vi.mocked(api.unblockAutoTag).mockReset().mockResolvedValue(undefined);
});

describe.each(['compact', 'comfort'])('tag chips in the %s detail panel', (density) => {
  it('offers the menu only on a tag the app would assign itself and keeps one-click removal for the others', async () => {
    const { onRemoveTag } = renderPanel(density);
    await removeAuto();
    expect(api.listSuggestedAutoTags).toHaveBeenCalledWith('model-1');
    const plain = screen.getByRole('button', { name: de.chipRemove.replace('{label}', 'mein-projekt') });
    expect(plain).not.toHaveAttribute('aria-haspopup');
    fireEvent.click(plain);
    expect(onRemoveTag).toHaveBeenCalledWith('mein-projekt');
    expect(screen.queryByRole('menu')).toBeNull();
    expect(api.blockAutoTag).not.toHaveBeenCalled();
  });

  it('keeps plain removal when the suggestions cannot be loaded', async () => {
    vi.mocked(api.listSuggestedAutoTags).mockRejectedValue(new Error('x'));
    const { onRemoveTag } = renderPanel(density);
    const button = await screen.findByRole('button', { name: de.chipRemove.replace('{label}', 'miniatur') });
    fireEvent.click(button);
    expect(onRemoveTag).toHaveBeenCalledWith('miniatur');
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('removes only: the menu entry behaves like the plain button and does not block', async () => {
    const { onRemoveTag } = renderPanel(density);
    fireEvent.click(await removeAuto());
    fireEvent.click(screen.getByRole('menuitem', { name: de.autoTagRemoveOnly }));
    expect(onRemoveTag).toHaveBeenCalledWith('miniatur');
    expect(api.blockAutoTag).not.toHaveBeenCalled();
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('removes and blocks, confirms in a status message and takes the block back with Undo', async () => {
    const { onRemoveTag, onAddTag } = renderPanel(density);
    fireEvent.click(await removeAuto());
    fireEvent.click(screen.getByRole('menuitem', { name: de.autoTagRemoveAndBlock }));
    await waitFor(() => expect(api.blockAutoTag).toHaveBeenCalledWith('miniatur'));
    expect(onRemoveTag).toHaveBeenCalledWith('miniatur');
    const status = screen.getByRole('status');
    await waitFor(() => expect(status).toHaveTextContent('„#miniatur“ wird nicht mehr automatisch vergeben.'));
    const undo = within(status).getByRole('button', { name: de.undoButton });
    expect(undo).toHaveFocus();

    fireEvent.click(undo);
    await waitFor(() => expect(api.unblockAutoTag).toHaveBeenCalledWith('miniatur'));
    expect(onAddTag).toHaveBeenCalledWith('miniatur');
    await waitFor(() => expect(status).toHaveTextContent('Sperre für „#miniatur“ aufgehoben'));
    expect(within(status).queryByRole('button')).toBeNull();
    expect(status).toHaveFocus();
  });

  it('does not remove the tag when blocking fails and says so', async () => {
    vi.mocked(api.blockAutoTag).mockRejectedValue(new Error('Datenbank gesperrt'));
    const { onRemoveTag } = renderPanel(density);
    fireEvent.click(await removeAuto());
    fireEvent.click(screen.getByRole('menuitem', { name: de.autoTagRemoveAndBlock }));
    expect(await screen.findByRole('alert')).toHaveTextContent(de.autoTagBlockError);
    expect(onRemoveTag).not.toHaveBeenCalled();
    expect(await removeAuto()).toHaveFocus();
  });

  it('opens with ArrowDown, moves with the arrow keys and closes with Escape back onto the button', async () => {
    renderPanel(density);
    const button = await removeAuto();
    button.focus();
    fireEvent.keyDown(button, { key: 'ArrowDown' });
    const [first, second] = screen.getAllByRole('menuitem');
    expect(button).toHaveAttribute('aria-expanded', 'true');
    expect(first).toHaveFocus();
    fireEvent.keyDown(first, { key: 'ArrowDown' });
    expect(second).toHaveFocus();
    fireEvent.keyDown(second, { key: 'ArrowDown' });
    expect(first).toHaveFocus();
    fireEvent.keyDown(first, { key: 'ArrowUp' });
    expect(second).toHaveFocus();
    fireEvent.keyDown(second, { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(button).toHaveFocus();
    expect(button).toHaveAttribute('aria-expanded', 'false');
  });

  it('describes both choices for screen readers', async () => {
    renderPanel(density);
    fireEvent.click(await removeAuto());
    expect(screen.getByRole('menu')).toHaveAccessibleName('miniatur entfernen: Auswahl');
    expect(screen.getByRole('menuitem', { name: de.autoTagRemoveAndBlock })).toHaveAccessibleDescription(de.autoTagRemoveAndBlockHint);
    expect(screen.getByRole('menuitem', { name: de.autoTagRemoveOnly })).toHaveAccessibleDescription(de.autoTagRemoveOnlyHint);
  });
});

it('offers no block in the trash panel', async () => {
  localStorage.setItem('3mf-katalog-density', 'comfort');
  render(<LanguageProvider><UiDensityProvider><DetailPanel model={makeModelFile({ tags: ['miniatur'], deletedAt: '2026-10-01T00:00:00Z' })} allTags={[]} trashMode
    onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop}
    onToggleQueue={noop} onUploadImage={noop} onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop}
    slicerError={null} /></UiDensityProvider></LanguageProvider>);
  await act(async () => {});
  expect(api.listSuggestedAutoTags).not.toHaveBeenCalled();
});

it('offers the same menu on the full detail page', async () => {
  const onRemoveTag = vi.fn();
  render(<LanguageProvider><ModelDetailPage model={model} allTags={[]} onClose={noop} hasPrevious={false} hasNext={false} onNavigate={noop} onAddTag={noop} onRemoveTag={onRemoveTag}
    onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop} onToggleQueue={noop} onUploadImage={noop}
    onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop} onRescanMetadata={noop} onAddToCollection={noop}
    collections={[]} slicers={[]} slicerError={null} rescanError={null} rescanSuccess={false} displayPreference="thumbnail" /></LanguageProvider>);
  fireEvent.click(await removeAuto());
  fireEvent.click(screen.getByRole('menuitem', { name: de.autoTagRemoveAndBlock }));
  await waitFor(() => expect(api.blockAutoTag).toHaveBeenCalledWith('miniatur'));
  expect(onRemoveTag).toHaveBeenCalledWith('miniatur');
  expect(screen.getByRole('button', { name: de.chipRemove.replace('{label}', 'mein-projekt') })).not.toHaveAttribute('aria-haspopup');
});
