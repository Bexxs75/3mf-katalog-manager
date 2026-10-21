import { invoke } from '@tauri-apps/api/core';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CompanionSection } from './CompanionSection';
import { DetailPanel } from './DetailPanel';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { makeModelFile } from '../test/factories';
import type { Companion } from '../types';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (s: string) => s }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

const invokeMock = vi.mocked(invoke);

function companion(overrides: Partial<Companion>): Companion {
  return { id: 'c1', fileId: 'model-1', name: 'Anleitung.pdf', path: '/catalog/Test_extras/Anleitung.pdf', kind: 'pdf',
    sizeBytes: 2048, addedAt: '2026-10-01T00:00:00Z', deletedAt: null, missing: false, ...overrides };
}

const photo = companion({ id: 'img', name: 'Foto.png', kind: 'image', path: '/c/Foto.png' });
const pdf = companion({});
const note = companion({ id: 'txt', name: 'Hinweise.txt', kind: 'text', sizeBytes: 12 });

let list: Companion[];
let handlers: Record<string, (args: any) => unknown>;

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  list = [photo, pdf, note];
  handlers = {};
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    if (handlers[command]) return handlers[command](args);
    if (command === 'list_companions') return list;
    if (command === 'companion_image_data') return 'data:image/png;base64,AAAA';
    return undefined;
  });
});

function renderSection(options: { lock?: boolean; container?: boolean } = {}) {
  return render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: options.container ?? false }}>
    <ImportLockContext.Provider value={options.lock ?? false}>
      <CompanionSection fileId="model-1" variant="page" />
    </ImportLockContext.Provider>
  </RuntimeEnvironmentProvider></LanguageProvider>);
}
const calls = (command: string) => invokeMock.mock.calls.filter(([name]) => name === command);

describe('CompanionSection', () => {
  it('shows pictures as named buttons with a lazily loaded preview and files as rows', async () => {
    renderSection();
    expect(await screen.findByRole('heading', { name: 'Begleitdateien (3)' })).toBeVisible();
    const tile = screen.getByRole('button', { name: 'Bild Foto.png groß anzeigen' });
    await waitFor(() => expect(tile.querySelector('img')).toHaveAttribute('src', 'data:image/png;base64,AAAA'));
    expect(invokeMock).toHaveBeenCalledWith('companion_image_data', { companionId: 'img' });
    expect(calls('companion_image_data')).toHaveLength(1);
    expect(screen.getByText('Anleitung.pdf')).toBeVisible();
    expect(screen.getByText('2 KB')).toBeVisible();
    expect(screen.getByText('Hinweise.txt')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Anleitung.pdf öffnen' })).toBeVisible();
  });

  it('shows a quiet empty state with the add button', async () => {
    list = [];
    renderSection();
    expect(await screen.findByText('Keine Begleitdateien.')).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Begleitdateien' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Hinzufügen …' })).toBeEnabled();
  });

  it('ignores companions in the trash', async () => {
    list = [companion({ deletedAt: '2026-10-02T00:00:00Z' })];
    renderSection();
    expect(await screen.findByText('Keine Begleitdateien.')).toBeVisible();
    expect(screen.queryByText('Anleitung.pdf')).toBeNull();
  });

  it('offers only "Entfernen" for a missing file', async () => {
    list = [companion({ missing: true })];
    renderSection();
    expect(await screen.findByText('Datei fehlt')).toBeVisible();
    expect(screen.queryByRole('button', { name: /öffnen/ })).toBeNull();
    expect(screen.queryByRole('button', { name: /Weitere Aktionen/ })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Entfernen: Anleitung.pdf' }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('remove_companion', { companionId: 'c1' }));
  });

  it('wraps long names instead of truncating them', async () => {
    const name = 'a-very-long-name-'.repeat(8) + '.pdf';
    list = [companion({ name })];
    renderSection();
    const text = await screen.findByText(name);
    expect(text.className).toContain('break-words');
    expect(text.className).not.toContain('truncate');
  });

  it('adds every picked file, reloads and moves focus to the add button', async () => {
    list = [];
    handlers.pick_companion_files = () => ['/home/u/a.png', '/home/u/b.pdf'];
    handlers.add_companion = (args) => { list = [...list, companion({ id: args.sourcePath, name: args.sourcePath.split('/').pop() })]; return list[list.length - 1]; };
    renderSection();
    await screen.findByText('Keine Begleitdateien.');
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen …' }));
    expect(await screen.findByText('2 Begleitdateien hinzugefügt.')).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith('add_companion', { fileId: 'model-1', sourcePath: '/home/u/a.png' });
    expect(invokeMock).toHaveBeenCalledWith('add_companion', { fileId: 'model-1', sourcePath: '/home/u/b.pdf' });
    expect(calls('list_companions').length).toBeGreaterThanOrEqual(2);
    expect(await screen.findByRole('heading', { name: 'Begleitdateien (2)' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Hinzufügen …' })).toHaveFocus();
  });

  it('does nothing when the file dialog is cancelled', async () => {
    handlers.pick_companion_files = () => [];
    renderSection();
    await screen.findByText('Anleitung.pdf');
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen …' }));
    await waitFor(() => expect(calls('pick_companion_files')).toHaveLength(1));
    expect(calls('add_companion')).toHaveLength(0);
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('reports each failed file by name and still adds the others', async () => {
    list = [];
    handlers.pick_companion_files = () => ['/x/tool.exe', '/x/ok.png', '/x/riesig.pdf'];
    handlers.add_companion = (args) => {
      if (args.sourcePath.endsWith('tool.exe')) return Promise.reject({ message: 'Dieser Dateityp ist als Begleitdatei nicht erlaubt.', expected: true });
      if (args.sourcePath.endsWith('riesig.pdf')) return Promise.reject({ message: 'Die Datei ist zu groß (höchstens 50 MB).', expected: true });
      list = [companion({ id: 'ok', name: 'ok.png', kind: 'image' })]; return list[0];
    };
    renderSection();
    await screen.findByText('Keine Begleitdateien.');
    fireEvent.click(screen.getByRole('button', { name: 'Hinzufügen …' }));
    const alert = await screen.findByRole('alert');
    expect(within(alert).getByText(/Nicht hinzugefügt: tool\.exe:/)).toBeVisible();
    expect(within(alert).getByText('Dieser Dateityp ist als Begleitdatei nicht erlaubt.')).toBeVisible();
    expect(within(alert).getByText(/Nicht hinzugefügt: riesig\.pdf:/)).toBeVisible();
    expect(within(alert).getByText('Die Datei ist zu groß (höchstens 50 MB).')).toBeVisible();
    expect(screen.getByText('1 Begleitdatei hinzugefügt.')).toBeVisible();
  });

  it('removes through the menu, reloads, announces it and keeps focus on a live control', async () => {
    handlers.remove_companion = () => { list = list.filter((item) => item.id !== 'c1'); };
    renderSection();
    await screen.findByText('Anleitung.pdf');
    fireEvent.click(screen.getByRole('button', { name: 'Weitere Aktionen für Anleitung.pdf' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Entfernen' }));
    expect(await screen.findByText('„Anleitung.pdf“ wurde in den Papierkorb gelegt.')).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith('remove_companion', { companionId: 'c1' });
    await waitFor(() => expect(screen.queryByText('Anleitung.pdf')).toBeNull());
    expect(screen.getByRole('button', { name: 'Hinzufügen …' })).toHaveFocus();
  });

  it('opens with the default program and shows the container block as a message', async () => {
    handlers.open_companion = () => Promise.reject({ message: 'containerExternalUnavailable', expected: true });
    renderSection({ container: true });
    await screen.findByText('Anleitung.pdf');
    fireEvent.click(screen.getByRole('button', { name: 'Anleitung.pdf öffnen' }));
    expect(invokeMock).toHaveBeenCalledWith('open_companion', { companionId: 'c1' });
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(de.containerExternalUnavailable);
  });

  it('reveals only the path from the open info, and not inside a container', async () => {
    handlers.companion_open_info = () => ({ id: 'c1', name: 'Anleitung.pdf', path: '/catalog/Test_extras/Anleitung.pdf', kind: 'pdf' });
    const { unmount } = renderSection();
    await screen.findByText('Anleitung.pdf');
    fireEvent.click(screen.getByRole('button', { name: 'Weitere Aktionen für Anleitung.pdf' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Im Dateimanager zeigen' }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('open_in_file_manager', { path: '/catalog/Test_extras/Anleitung.pdf' }));
    expect(invokeMock).toHaveBeenCalledWith('companion_open_info', { companionId: 'c1' });
    unmount();
    renderSection({ container: true });
    await screen.findByText('Anleitung.pdf');
    fireEvent.click(screen.getByRole('button', { name: 'Weitere Aktionen für Anleitung.pdf' }));
    expect(screen.queryByRole('menuitem', { name: 'Im Dateimanager zeigen' })).toBeNull();
    expect(screen.getByRole('menuitem', { name: 'Entfernen' })).toBeVisible();
  });

  it('closes the menu with Escape and returns focus to its button', async () => {
    renderSection();
    await screen.findByText('Anleitung.pdf');
    const trigger = screen.getByRole('button', { name: 'Weitere Aktionen für Anleitung.pdf' });
    fireEvent.click(trigger);
    expect(screen.getByRole('menu')).toBeVisible();
    fireEvent.keyDown(screen.getByRole('menuitem', { name: 'Im Dateimanager zeigen' }), { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it('opens the enlarged picture as a dialog, closes it with Escape and returns focus', async () => {
    renderSection();
    const tile = await screen.findByRole('button', { name: 'Bild Foto.png groß anzeigen' });
    await waitFor(() => expect(tile.querySelector('img')).not.toBeNull());
    tile.focus();
    fireEvent.click(tile);
    const dialog = await screen.findByRole('dialog', { name: 'Foto.png' });
    expect(within(dialog).getByRole('img', { name: 'Foto.png' })).toHaveAttribute('src', 'data:image/png;base64,AAAA');
    expect(within(dialog).getByRole('button', { name: 'Großansicht schließen' })).toHaveFocus();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(tile).toHaveFocus();
  });

  it('locks add and remove during an import but still allows opening', async () => {
    renderSection({ lock: true });
    await screen.findByText('Anleitung.pdf');
    expect(screen.getByRole('button', { name: 'Hinzufügen …' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Weitere Aktionen für Anleitung.pdf' }));
    expect(screen.getByRole('menuitem', { name: 'Entfernen' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Anleitung.pdf öffnen' })).toBeEnabled();
  });

  it('shows a load error as an alert', async () => {
    handlers.list_companions = () => Promise.reject({ message: 'Datenbank kaputt', expected: true });
    renderSection();
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Die Begleitdateien konnten nicht geladen werden.');
    expect(alert).toHaveTextContent('Datenbank kaputt');
    expect(screen.queryByText('Keine Begleitdateien.')).toBeNull();
  });

  it('renders names only as text', async () => {
    list = [companion({ name: '<img src=x onerror=alert(1)>.pdf' })];
    const { container } = renderSection();
    await screen.findByText('<img src=x onerror=alert(1)>.pdf');
    expect(container.querySelector('img[src="x"]')).toBeNull();
  });
});

describe('DetailPanel', () => {
  const noop = vi.fn();
  const panel = (density: string, trashMode = false) => {
    localStorage.setItem('3mf-katalog-density', density);
    return render(<LanguageProvider><UiDensityProvider><DetailPanel model={makeModelFile()} allTags={[]} trashMode={trashMode}
      onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop}
      onToggleQueue={noop} onUploadImage={noop} onSnapshotCaptured={noop} onSetSourceUrl={noop}
      onOpenInSlicer={noop} slicerError={null} /></UiDensityProvider></LanguageProvider>);
  };

  it.each(['compact', 'comfort'])('shows the companion section in the %s panel', async (density) => {
    panel(density);
    expect(await screen.findByRole('heading', { name: 'Begleitdateien (3)' })).toBeVisible();
    expect(invokeMock).toHaveBeenCalledWith('list_companions', { fileId: 'model-1' });
  });

  it.each(['compact', 'comfort'])('leaves the section out in trash mode (%s)', async (density) => {
    panel(density, true);
    await act(async () => { await Promise.resolve(); });
    expect(screen.queryByRole('heading', { name: /Begleitdateien/ })).toBeNull();
    expect(calls('list_companions')).toHaveLength(0);
    expect(screen.queryByRole('button', { name: 'Hinzufügen …' })).toBeNull();
  });
});

describe('texts', () => {
  const keys = ['companionsHeading', 'companionsEmpty', 'companionsAdd', 'companionOpen', 'companionOpenNamed', 'companionMenu',
    'companionReveal', 'companionRemove', 'companionMissing', 'companionViewImage', 'companionPreviewClose',
    'companionPreviewUnavailable', 'companionRemoved', 'companionAddFailed', 'companionsLoadFailed'] as const;
  it.each([['de', de], ['en', en], ['es', es], ['fr', fr]] as const)('has all companion texts in %s', (_name, table) => {
    for (const key of keys) expect(String(table[key]).length, key).toBeGreaterThan(1);
    for (const key of ['companionsAdded', 'archiveSummaryCompanionsAssigned', 'archiveSummaryCompanionsIgnored'] as const) {
      expect(table[key].one).toContain('{count}');
      expect(table[key].other).toContain('{count}');
    }
    for (const key of ['companionOpenNamed', 'companionMenu', 'companionViewImage', 'companionRemoved', 'companionAddFailed'] as const) {
      expect(table[key], key).toContain('{name}');
    }
  });
  it('differs between the four languages for the heading', () => {
    expect(new Set([de, en, es, fr].map((table) => table.companionsHeading)).size).toBe(4);
  });
});
