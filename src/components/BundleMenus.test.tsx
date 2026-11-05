import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ContextMenu, type BundleMenuActions } from './ContextMenu';
import { BundleContextMenu } from './BundleContextMenu';
import { BulkActionToolbar, type BulkBundleActions } from './BulkActionToolbar';
import { SelectionAnnouncer } from './SelectionAnnouncer';
import { clickBulkMore, openBulkMore } from '../test/bulkBar';
import { de } from '../i18n/de';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const noop = () => {};
function fileMenu(bundleActions?: BundleMenuActions) {
  return render(<LanguageProvider>
    <ContextMenu x={0} y={0} onClose={noop} onOpenInSlicer={noop} onDelete={noop} inQueue={false} onToggleQueue={noop} printed={false}
      onTogglePrintStatus={noop} fileId="1" currentName="a.3mf" onRename={async () => {}} bundleActions={bundleActions} />
  </LanguageProvider>);
}

describe('file context menu, bundle entries', () => {
  it('offers only the actions that apply and closes after one', () => {
    const onClose = vi.fn();
    const onCreate = vi.fn();
    render(<LanguageProvider>
      <ContextMenu x={0} y={0} onClose={onClose} onOpenInSlicer={noop} onDelete={noop} inQueue={false} onToggleQueue={noop} printed={false}
        onTogglePrintStatus={noop} fileId="1" currentName="a.3mf" onRename={async () => {}} bundleActions={{ onCreate, onRemove: undefined, onAddTo: undefined }} />
    </LanguageProvider>);
    expect(screen.queryByRole('button', { name: de.bundleAddTo })).toBeNull();
    expect(screen.queryByRole('button', { name: de.bundleRemoveFrom })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: de.bundleMake }));
    expect(onCreate).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalled();
  });

  it('shows no bundle entry when no action applies', () => {
    fileMenu({});
    for (const label of [de.bundleMake, de.bundleAddTo, de.bundleRemoveFrom]) expect(screen.queryByRole('button', { name: label })).toBeNull();
  });
});

describe('bundle card context menu', () => {
  const props = {
    x: 0, y: 0, count: 3, allPrinted: false, allFavorite: false, canQueue: true, canDelete: true, onClose: noop,
    onEdit: vi.fn(), onTogglePrinted: vi.fn(), onQueueAll: vi.fn(), onToggleFavorite: vi.fn(), onDissolve: vi.fn(), onDelete: vi.fn(),
  };
  const menu = (extra: Partial<typeof props> = {}) => render(<LanguageProvider><BundleContextMenu {...props} {...extra} /></LanguageProvider>);

  it('names the number of models in the entries that work on all of them and focuses the first', () => {
    menu();
    expect(screen.getByRole('button', { name: 'Alle 3 als gedruckt markieren' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Alle 3 zur Warteschlange hinzufügen' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Alle 3 als Favoriten markieren' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.bundleEdit })).toHaveFocus();
  });

  it('turns the entries around when every member already has the state', () => {
    menu({ allPrinted: true, allFavorite: true });
    expect(screen.getByRole('button', { name: 'Alle 3 als nicht gedruckt markieren' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Alle 3 aus den Favoriten nehmen' })).toBeInTheDocument();
  });

  it('leaves out the queue entry for variants and the delete entry without the right to delete', () => {
    menu({ canQueue: false, canDelete: false });
    expect(screen.queryByRole('button', { name: /Warteschlange/ })).toBeNull();
    expect(screen.queryByRole('button', { name: de.bundleDeleteEntry })).toBeNull();
    expect(screen.getByRole('button', { name: de.bundleDissolve })).toBeInTheDocument();
  });

  it('runs the action of the clicked entry', () => {
    menu();
    fireEvent.click(screen.getByRole('button', { name: de.bundleDissolve }));
    expect(props.onDissolve).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('button', { name: de.bundleDeleteEntry }));
    expect(props.onDelete).toHaveBeenCalledOnce();
  });
});

describe('selection bar, bundle entries', () => {
  const actions = (extra: Partial<BulkBundleActions> = {}): BulkBundleActions => ({
    canCreate: true, canAddTo: true, canRemove: true, canDissolve: true, canEdit: true,
    onCreate: vi.fn(), onAddTo: vi.fn(), onRemove: vi.fn(), onDissolve: vi.fn(), onEdit: vi.fn(), ...extra,
  });
  function bar(bundleActions?: BulkBundleActions) {
    return render(<LanguageProvider>
      <BulkActionToolbar selectedCount={2} confirmBulkDelete={false} onConfirmBulkDeleteChange={noop} onSelectAllVisible={noop} onClearSelection={noop}
        onBulkAddToQueue={noop} addToCollectionMenuOpen={false} onAddToCollectionMenuOpenChange={noop} collections={[]} onBulkAddToCollection={noop}
        activeCollection={null} onBulkRemoveFromCollection={noop} onBulkSetPrintStatus={noop} onBulkDelete={noop} addTagMenuOpen={false}
        onAddTagMenuOpenChange={noop} allTags={[]} tagsInAllSelection={[]} tagDraft="" onTagDraftChange={noop} onSubmitBulkAddTag={noop}
        removeTagMenuOpen={false} onRemoveTagMenuOpenChange={noop} tagsInSelection={[]} onBulkRemoveTag={noop} bundleActions={bundleActions} />
    </LanguageProvider>);
  }

  it('shows no bundle entry without bundle actions', () => {
    bar();
    const menu = openBulkMore();
    expect(within(menu).queryByRole('menuitem', { name: de.bundleMake })).toBeNull();
  });

  it('lists the entries that apply and passes the "More" button as the place focus returns to', () => {
    const a = actions();
    bar(a);
    clickBulkMore(de.bundleMake);
    const more = screen.getByRole('button', { name: /^Mehr/ });
    expect(a.onCreate).toHaveBeenCalledWith(more);
    clickBulkMore(de.bundleAddTo);
    expect(a.onAddTo).toHaveBeenCalledWith(more);
    clickBulkMore(de.bundleEdit);
    expect(a.onEdit).toHaveBeenCalledWith(more);
    clickBulkMore(de.bundleRemoveFrom);
    expect(a.onRemove).toHaveBeenCalledOnce();
    clickBulkMore(de.bundleDissolve);
    expect(a.onDissolve).toHaveBeenCalledOnce();
  });

  it('hides what does not apply and disables "Bundle …" with the reason when fewer than two models are selected', () => {
    bar(actions({ canCreate: false, canAddTo: false, canRemove: false, canDissolve: false, canEdit: false }));
    const menu = openBulkMore();
    const make = within(menu).getByRole('menuitem', { name: de.bundleMake });
    expect(make).toBeDisabled();
    expect(make).toHaveAttribute('title', de.bundleDlgNeedTwo);
    for (const label of [de.bundleAddTo, de.bundleEdit, de.bundleRemoveFrom, de.bundleDissolve]) {
      expect(within(menu).queryByRole('menuitem', { name: label })).toBeNull();
    }
  });
});

describe('SelectionAnnouncer', () => {
  const announcer = () => document.querySelector('[data-selection-announcer]') as HTMLElement;
  const wrap = (count: number, bundleCount?: number) => <LanguageProvider><SelectionAnnouncer count={count} bundleCount={bundleCount} /></LanguageProvider>;

  it('names the bundles next to the models', () => {
    const view = render(wrap(0));
    view.rerender(wrap(7, 2));
    expect(announcer()).toHaveTextContent('2 Bündel, 7 Modelle ausgewählt');
  });

  it('says it again when only the number of bundles changed', () => {
    const view = render(wrap(0));
    view.rerender(wrap(7, 2));
    view.rerender(wrap(7, 1));
    expect(announcer()).toHaveTextContent('1 Bündel, 7 Modelle ausgewählt');
  });

  it('keeps the old wording without bundles', () => {
    const view = render(wrap(0));
    view.rerender(wrap(3, 0));
    expect(announcer()).toHaveTextContent('3 Modelle ausgewählt');
    act(() => { view.rerender(wrap(0, 0)); });
    expect(announcer()).toHaveTextContent(de.selectionCleared);
  });
});
