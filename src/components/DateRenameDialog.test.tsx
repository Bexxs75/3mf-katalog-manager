import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DateRenameDialog } from './DateRenameDialog';
import { DateRenameBanner } from './DateRenameBanner';
import { BulkActionToolbar } from './BulkActionToolbar';
import { clickBulkMore, openBulkMore } from '../test/bulkBar';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';
import type { DateRenamePreview } from '../lib/api/files';

const preview = vi.fn();
const apply = vi.fn();
vi.mock('../lib/api/files', async importOriginal => ({
  ...(await importOriginal<typeof import('../lib/api/files')>()),
  previewDateRename: (...args: unknown[]) => preview(...args),
  applyDateRename: (...args: unknown[]) => apply(...args),
}));

const rows = (prefix: string): DateRenamePreview[] => [
  { id: '1', oldName: 'Halter.3mf', newName: `${prefix}Halter.3mf`, status: 'rename' },
  { id: '2', oldName: 'Clip.stl', newName: `${prefix}Clip.stl`, status: 'rename' },
  { id: '3', oldName: '2026-10-01_08-00_Vase.3mf', newName: '2026-10-01_08-00_Vase.3mf', status: 'skip_has_prefix' },
  { id: '4', oldName: 'Schale.3mf', newName: 'Schale.3mf', status: 'skip_conflict' },
];

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
  preview.mockReset().mockImplementation(async (_ids: string[], source: string) =>
    rows(source === 'modified' ? '2026-09-12_14-03_' : '2025-01-01_00-00_'));
  apply.mockReset().mockResolvedValue([
    { id: '1', status: 'renamed' }, { id: '2', status: 'renamed' },
  ]);
});

function open(props: Partial<React.ComponentProps<typeof DateRenameDialog>> = {}) {
  const onClose = vi.fn();
  const onDone = vi.fn();
  render(<LanguageProvider><DateRenameDialog fileIds={['1', '2', '3', '4']} onClose={onClose} onDone={onDone} {...props} /></LanguageProvider>);
  return { onClose, onDone };
}

describe('DateRenameDialog', () => {
  it('previews with the file date first and preselects only the rename rows', async () => {
    open();
    expect(await screen.findByText('2026-09-12_14-03_Halter.3mf')).toBeTruthy();
    expect(preview).toHaveBeenCalledWith(['1', '2', '3', '4'], 'modified');
    expect(screen.getByText('4 ausgewählte Modelle', { exact: false })).toBeTruthy();
    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[];
    expect(boxes.map(box => box.checked)).toEqual([true, true, false, false]);
    expect(boxes.map(box => box.disabled)).toEqual([false, false, true, true]);
    expect(screen.getByText('(hat schon ein Datum, übersprungen)')).toBeTruthy();
    expect(screen.getByText('(Zielname ist schon vergeben)').className).toContain('red');
    expect(screen.getByRole('button', { name: '2 Dateien umbenennen' })).toBeTruthy();
  });

  it('reloads the preview when the date source changes', async () => {
    open();
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    const imported = screen.getByRole('radio', { name: 'Importdatum' });
    imported.focus();
    fireEvent.click(imported);
    expect(await screen.findByText('2025-01-01_00-00_Halter.3mf')).toBeTruthy();
    expect(preview).toHaveBeenLastCalledWith(['1', '2', '3', '4'], 'imported');
    expect(screen.getByRole('radio', { name: 'Importdatum' })).toBe(document.activeElement);
  });

  it('sends only the checked rows with their previewed names', async () => {
    const { onDone } = open();
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    fireEvent.click(screen.getByRole('checkbox', { name: /Clip\.stl/ }));
    fireEvent.click(screen.getByRole('button', { name: '1 Datei umbenennen' }));
    await waitFor(() => expect(apply).toHaveBeenCalledTimes(1));
    expect(apply).toHaveBeenCalledWith([{ id: '1', newName: '2026-09-12_14-03_Halter.3mf' }]);
    await waitFor(() => expect(onDone).toHaveBeenCalledWith({ renamed: 2, failed: 0 }));
  });

  it('reports partial failures to the caller', async () => {
    apply.mockResolvedValue([{ id: '1', status: 'renamed' }, { id: '2', status: 'failed', reason: 'targetTaken' }]);
    const { onDone } = open();
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    fireEvent.click(screen.getByRole('button', { name: '2 Dateien umbenennen' }));
    await waitFor(() => expect(onDone).toHaveBeenCalledWith({ renamed: 1, failed: 1 }));
  });

  it('keeps the dialog open and shows the error when applying throws', async () => {
    apply.mockRejectedValue(new Error('boom'));
    const { onDone } = open();
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    fireEvent.click(screen.getByRole('button', { name: '2 Dateien umbenennen' }));
    expect((await screen.findByRole('alert')).textContent).toContain('boom');
    expect(onDone).not.toHaveBeenCalled();
    expect((screen.getByRole('button', { name: '2 Dateien umbenennen' }) as HTMLButtonElement).disabled).toBe(false);
  });

  it('disables the confirm button without a selection and during a running import', async () => {
    const view = render(
      <LanguageProvider><ImportLockContext.Provider value><DateRenameDialog fileIds={['1']} onClose={vi.fn()} onDone={vi.fn()} /></ImportLockContext.Provider></LanguageProvider>);
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    expect((screen.getByRole('button', { name: /Dateien umbenennen|Datei umbenennen/ }) as HTMLButtonElement).disabled).toBe(true);
    view.unmount();
    open();
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    fireEvent.click(screen.getByRole('checkbox', { name: /Halter/ }));
    fireEvent.click(screen.getByRole('checkbox', { name: /Clip/ }));
    expect((screen.getByRole('button', { name: '0 Dateien umbenennen' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('closes with Escape and traps Tab inside the dialog', async () => {
    const { onClose } = open();
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    const dialog = screen.getByRole('dialog', { name: 'Dateien mit Datum umbenennen' });
    expect(dialog.contains(document.activeElement)).toBe(true);
    const buttons = within(dialog).getAllByRole('button');
    buttons[buttons.length - 1].focus();
    // Tab on the last control wraps to the first; Shift+Tab on the first wraps back.
    fireEvent.keyDown(document, { key: 'Tab' });
    expect(document.activeElement).toBe(within(dialog).getAllByRole('radio')[0]);
    fireEvent.keyDown(document, { key: 'Tab', shiftKey: true });
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).not.toBe(document.body);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('ignores a late answer for a previous date source', async () => {
    let releaseFirst: (value: DateRenamePreview[]) => void = () => {};
    preview.mockReset()
      .mockImplementationOnce(() => new Promise(resolve => { releaseFirst = resolve; }))
      .mockImplementationOnce(async () => rows('2025-01-01_00-00_'));
    open();
    fireEvent.click(screen.getByRole('radio', { name: 'Importdatum' }));
    expect(await screen.findByText('2025-01-01_00-00_Halter.3mf')).toBeTruthy();
    await act(async () => releaseFirst(rows('2026-09-12_14-03_')));
    expect(screen.queryByText('2026-09-12_14-03_Halter.3mf')).toBeNull();
  });

  it('refuses oversized selections without calling the backend', async () => {
    open({ fileIds: Array.from({ length: 5001 }, (_, n) => String(n)) });
    expect((await screen.findByRole('alert')).textContent).toContain('5000');
    expect(preview).not.toHaveBeenCalled();
  });

  it('shows a preview error', async () => {
    preview.mockReset().mockRejectedValue(new Error('kaputt'));
    open();
    expect((await screen.findByRole('alert')).textContent).toContain('kaputt');
  });
});

describe('date rename entry in the selection bar', () => {
  function renderBar(onDateRenamed = vi.fn()) {
    const noop = vi.fn();
    render(
      <LanguageProvider>
        <BulkActionToolbar selectedCount={2} confirmBulkDelete={false} onConfirmBulkDeleteChange={noop} onSelectAllVisible={noop}
          onClearSelection={noop} onBulkAddToQueue={noop} addToCollectionMenuOpen={false} onAddToCollectionMenuOpenChange={noop}
          collections={[]} onBulkAddToCollection={noop} activeCollection={null} onBulkRemoveFromCollection={noop}
          onBulkSetPrintStatus={noop} onBulkDelete={noop} addTagMenuOpen={false} onAddTagMenuOpenChange={noop} allTags={[]}
          tagsInAllSelection={[]} tagDraft="" onTagDraftChange={noop} onSubmitBulkAddTag={noop} removeTagMenuOpen={false}
          onRemoveTagMenuOpenChange={noop} tagsInSelection={[]} onBulkRemoveTag={noop}
          selectedIds={['1', '2', '3', '4']} onDateRenamed={onDateRenamed} />
      </LanguageProvider>);
    return { noop, onDateRenamed };
  }

  it('opens the dialog, returns focus to the trigger on Escape and keeps the selection', async () => {
    const { noop } = renderBar();
    // The entry lives in the "More" menu; focus returns to the button that opened it.
    const trigger = screen.getByRole('button', { name: /^Mehr/ });
    clickBulkMore('Mit Datum umbenennen …');
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    fireEvent.keyDown(document, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(document.activeElement).toBe(trigger);
    expect(noop).not.toHaveBeenCalled();
  });

  it('closes the dialog and reports the outcome after applying', async () => {
    const { onDateRenamed } = renderBar();
    clickBulkMore('Mit Datum umbenennen …');
    await screen.findByText('2026-09-12_14-03_Halter.3mf');
    fireEvent.click(screen.getByRole('button', { name: '2 Dateien umbenennen' }));
    await waitFor(() => expect(onDateRenamed).toHaveBeenCalledWith({ renamed: 2, failed: 0 }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('is locked while an import runs', () => {
    cleanup();
    const noop = vi.fn();
    render(
      <LanguageProvider><ImportLockContext.Provider value>
        <BulkActionToolbar selectedCount={1} confirmBulkDelete={false} onConfirmBulkDeleteChange={noop} onSelectAllVisible={noop}
          onClearSelection={noop} onBulkAddToQueue={noop} addToCollectionMenuOpen={false} onAddToCollectionMenuOpenChange={noop}
          collections={[]} onBulkAddToCollection={noop} activeCollection={null} onBulkRemoveFromCollection={noop}
          onBulkSetPrintStatus={noop} onBulkDelete={noop} addTagMenuOpen={false} onAddTagMenuOpenChange={noop} allTags={[]}
          tagsInAllSelection={[]} tagDraft="" onTagDraftChange={noop} onSubmitBulkAddTag={noop} removeTagMenuOpen={false}
          onRemoveTagMenuOpenChange={noop} tagsInSelection={[]} onBulkRemoveTag={noop}
          selectedIds={['1']} onDateRenamed={noop} />
      </ImportLockContext.Provider></LanguageProvider>);
    expect((within(openBulkMore()).getByRole('menuitem', { name: 'Mit Datum umbenennen …' }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('DateRenameBanner and texts', () => {
  it('shows renamed and failed counts', () => {
    const view = render(<LanguageProvider><DateRenameBanner outcome={{ renamed: 3, failed: 1 }} onClose={vi.fn()} /></LanguageProvider>);
    expect(screen.getByRole('alert').textContent).toContain('3 Dateien umbenannt, 1 fehlgeschlagen');
    view.rerender(<LanguageProvider><DateRenameBanner outcome={{ renamed: 1, failed: 0 }} onClose={vi.fn()} /></LanguageProvider>);
    expect(within(screen.getByRole('status')).getByText('1 Datei umbenannt')).toBeTruthy();
  });

  it('has the texts in all four languages', () => {
    for (const dictionary of [de, en, es, fr]) {
      for (const [key, value] of Object.entries(dictionary).filter(([key]) => key.startsWith('dateRename'))) {
        const text = typeof value === 'string' ? value : `${value.one}${value.other}`;
        expect(text.trim(), key).not.toBe('');
      }
    }
    expect(en.dateRenameMenu).toBe('Rename with date …');
  });
});
