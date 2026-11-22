import { chooseSelect } from '../test/select';
import { modelMenuActions } from '../lib/modelMenuActions';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { BulkActionToolbar } from './BulkActionToolbar';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { clickBulkMore, openBulkMore } from '../test/bulkBar';

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
});

function renderToolbar(overrides: Partial<Parameters<typeof BulkActionToolbar>[0]> = {}) {
  const props = {
    selectedCount: 2,
    confirmBulkDelete: false,
    onConfirmBulkDeleteChange: vi.fn(),
    onSelectAllVisible: vi.fn(),
    onClearSelection: vi.fn(),
    onBulkAddToQueue: vi.fn(),
    onBulkToggleFavorite: vi.fn(),
    onBulkRemoveFromQueue: vi.fn(),
    addToCollectionMenuOpen: false,
    onAddToCollectionMenuOpenChange: vi.fn(),
    collections: [],
    onBulkAddToCollection: vi.fn(),
    activeCollection: null,
    onBulkRemoveFromCollection: vi.fn(),
    onBulkSetPrintStatus: vi.fn(),
    onBulkDelete: vi.fn(),
    onBulkRemove: vi.fn(async () => {}),
    addTagMenuOpen: false,
    onAddTagMenuOpenChange: vi.fn(),
    allTags: [],
    tagsInAllSelection: [],
    tagDraft: '',
    onTagDraftChange: vi.fn(),
    onSubmitBulkAddTag: vi.fn(),
    removeTagMenuOpen: false,
    onRemoveTagMenuOpenChange: vi.fn(),
    tagsInSelection: ['miniatur'],
    onBulkRemoveTag: vi.fn(),
    ...overrides,
  };
  const view = render(
    <LanguageProvider>
      <BulkActionToolbar {...props} />
    </LanguageProvider>,
  );
  return { props, view };
}

describe('BulkActionToolbar', () => {
  it.each([
    ['removeTagMenuOpen', 'onRemoveTagMenuOpenChange'],
    ['addTagMenuOpen', 'onAddTagMenuOpenChange'],
    ['addToCollectionMenuOpen', 'onAddToCollectionMenuOpenChange'],
  ] as const)('closes %s on the first Escape and clears on the second', (menu, onChange) => {
    const { props, view } = renderToolbar({ [menu]: true });
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props[onChange]).toHaveBeenCalledWith(false);
    expect(props.onBulkRemoveTag).not.toHaveBeenCalled();
    expect(props.onClearSelection).not.toHaveBeenCalled();
    view.rerender(
      <LanguageProvider>
        <BulkActionToolbar {...props} {...{ [menu]: false }} />
      </LanguageProvider>,
    );
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onClearSelection).toHaveBeenCalledTimes(1);
  });

  it('clears the selection once with Escape while no menu is open', () => {
    const { props } = renderToolbar();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onClearSelection).toHaveBeenCalledTimes(1);
    expect(props.onRemoveTagMenuOpenChange).not.toHaveBeenCalled();
    expect(props.onAddTagMenuOpenChange).not.toHaveBeenCalled();
    expect(props.onAddToCollectionMenuOpenChange).not.toHaveBeenCalled();
  });

  it('cancels bulk delete confirmation without clearing the selection', () => {
    const { props } = renderToolbar({ confirmBulkDelete: true });
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onConfirmBulkDeleteChange).toHaveBeenCalledExactlyOnceWith(false);
    expect(props.onBulkDelete).not.toHaveBeenCalled();
    expect(props.onClearSelection).not.toHaveBeenCalled();
  });

  it('cancels remove confirmation without clearing until the next Escape', () => {
    const { props } = renderToolbar();
    clickBulkMore('Aus Katalog entfernen');
    fireEvent.keyDown(screen.getByRole('button', { name: 'Abbrechen' }), { key: 'Escape' });
    expect(screen.getByRole('button', { name: 'Ablegen' })).toBeInTheDocument();
    expect(props.onClearSelection).not.toHaveBeenCalled();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onClearSelection).toHaveBeenCalledTimes(1);
  });

  it.each([
    ['an expected error is translated without the report link', { message: 'modelNotFoundOrTrashed', expected: true }, 'Modell nicht gefunden oder bereits im Papierkorb.', false],
    ['an unexpected error shows its text with the report link', { message: 'database is locked', expected: false }, 'database is locked', true],
  ])('shows %s', async (_name, rejection, text, report) => {
    const { props } = renderToolbar({ onBulkRemove: vi.fn(() => Promise.reject(rejection)) });
    clickBulkMore('Aus Katalog entfernen');
    fireEvent.click(screen.getByRole('button', { name: 'Entfernen' }));
    expect(await screen.findByText(text)).toBeInTheDocument();
    expect(props.onBulkRemove).toHaveBeenCalled();
    expect(!!screen.queryByText('Problem melden')).toBe(report);
  });

  it('cancels remove confirmation with focus outside the bar before clearing selection', () => {
    const { props } = renderToolbar();
    clickBulkMore('Aus Katalog entfernen');
    const outside = document.createElement('button');
    document.body.appendChild(outside);
    try {
      outside.focus();
      expect(outside).toHaveFocus();
      fireEvent.keyDown(outside, { key: 'Escape' });
      expect(screen.getByRole('button', { name: 'Ablegen' })).toBeInTheDocument();
      expect(props.onClearSelection).not.toHaveBeenCalled();
      expect(props.onBulkRemove).not.toHaveBeenCalled();
      fireEvent.keyDown(outside, { key: 'Escape' });
      expect(props.onClearSelection).toHaveBeenCalledTimes(1);
    } finally {
      outside.remove();
    }
  });

  it.each([
    ['delete', { confirmBulkDelete: true }],
    ['remove', {}],
  ] as const)('closes open menus when the %s confirmation opens', (kind, overrides) => {
    const { props } = renderToolbar({ ...overrides, removeTagMenuOpen: kind === 'delete' });
    if (kind === 'remove') {
      clickBulkMore('Aus Katalog entfernen');
    }
    expect(props.onRemoveTagMenuOpenChange).toHaveBeenCalledWith(false);
    expect(props.onAddTagMenuOpenChange).toHaveBeenCalledWith(false);
    expect(props.onAddToCollectionMenuOpenChange).toHaveBeenCalledWith(false);
    expect(props.onClearSelection).not.toHaveBeenCalled();
  });

  it('closes menus first, then the confirmation, then the selection, one per Escape', () => {
    const { props, view } = renderToolbar({ removeTagMenuOpen: true });
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onRemoveTagMenuOpenChange).toHaveBeenCalledWith(false);
    expect(props.onConfirmBulkDeleteChange).not.toHaveBeenCalled();
    view.rerender(
      <LanguageProvider>
        <BulkActionToolbar {...props} removeTagMenuOpen={false} confirmBulkDelete />
      </LanguageProvider>,
    );
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onConfirmBulkDeleteChange).toHaveBeenCalledExactlyOnceWith(false);
    expect(props.onClearSelection).not.toHaveBeenCalled();
    view.rerender(
      <LanguageProvider>
        <BulkActionToolbar {...props} removeTagMenuOpen={false} confirmBulkDelete={false} />
      </LanguageProvider>,
    );
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onClearSelection).toHaveBeenCalledTimes(1);
  });

  it('closes the tag input menu without clearing the selection', () => {
    const { props } = renderToolbar({ addTagMenuOpen: true });
    expect(screen.getByRole('combobox')).toHaveAttribute('maxlength', '100');
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Escape' });
    expect(props.onAddTagMenuOpenChange).toHaveBeenCalledWith(false);
    expect(props.onClearSelection).not.toHaveBeenCalled();
  });

  it.each(['input', 'textarea', 'contenteditable'])('ignores Escape from %s with no menu open', (kind) => {
    const { props } = renderToolbar();
    const editor = document.createElement(kind === 'contenteditable' ? 'div' : kind);
    if (kind === 'contenteditable') editor.setAttribute('contenteditable', 'true');
    const target = kind === 'contenteditable' ? editor.appendChild(document.createElement('span')) : editor;
    document.body.appendChild(editor);
    try {
      fireEvent.keyDown(target, { key: 'Escape' });
      expect(props.onClearSelection).not.toHaveBeenCalled();
    } finally {
      editor.remove();
    }
  });

  it('clears the selection with Escape while a card checkbox has focus', () => {
    const { props } = renderToolbar();
    const checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    document.body.appendChild(checkbox);
    try {
      checkbox.focus();
      fireEvent.keyDown(checkbox, { key: 'Escape' });
      expect(props.onClearSelection).toHaveBeenCalledTimes(1);
    } finally {
      checkbox.remove();
    }
  });

  it('wraps as a last resort so no action becomes unreachable in a very narrow window', () => {
    renderToolbar();
    expect(screen.getByRole('toolbar')).toHaveClass('flex-wrap');
  });
});

it('focuses Cancel in the inline remove confirmation and restores focus to the Ablegen button on Escape', () => {
  const { props } = renderToolbar();
  clickBulkMore('Aus Katalog entfernen');
  expect(screen.getByRole('button', { name: 'Abbrechen' })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
  // The entry sits in the "Ablegen" menu, so focus goes back to the button that opens it.
  expect(screen.getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  expect(props.onBulkRemove).not.toHaveBeenCalled();
});

it('suggests tags absent from at least one selected model and submits the highlighted tag', () => {
  const { props } = renderToolbar({ addTagMenuOpen: true, allTags: ['common', 'partial', 'unused'], tagsInAllSelection: ['common'] });
  const input = screen.getByRole('combobox');
  expect(input).toHaveFocus();
  expect(screen.queryByRole('option', { name: 'common' })).not.toBeInTheDocument();
  expect(screen.getByRole('option', { name: 'partial' })).toBeInTheDocument();
  expect(screen.getByRole('option', { name: 'unused' })).toBeInTheDocument();
  fireEvent.keyDown(input, { key: 'ArrowDown' });
  expect(input).toHaveAttribute('aria-activedescendant', screen.getByRole('option', { name: 'partial' }).id);
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(props.onSubmitBulkAddTag).toHaveBeenCalledWith('partial');
});

it('closes suggestions before the tag menu and restores trigger focus', () => {
  const { props } = renderToolbar({ addTagMenuOpen: true, allTags: ['partial'] });
  const input = screen.getByRole('combobox');
  fireEvent.keyDown(input, { key: 'Escape' });
  expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  expect(input).toHaveFocus();
  expect(props.onAddTagMenuOpenChange).not.toHaveBeenCalled();
  fireEvent.keyDown(input, { key: 'Escape' });
  expect(props.onAddTagMenuOpenChange).toHaveBeenCalledWith(false);
  expect(screen.getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  expect(props.onClearSelection).not.toHaveBeenCalled();
});

it('submits typed canonical tags with an optional hash instead of an unselected suggestion', () => {
  const { props } = renderToolbar({ addTagMenuOpen: true, tagDraft: ' #Multipart ', allTags: ['something'] });
  fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' });
  expect(props.onSubmitBulkAddTag).toHaveBeenCalledWith('mehrteilig');
});

it('selects the last suggestion with ArrowUp and accepts a clicked suggestion', () => {
  const { props } = renderToolbar({ addTagMenuOpen: true, allTags: ['alpha', 'beta'] });
  const input = screen.getByRole('combobox');
  fireEvent.keyDown(input, { key: 'ArrowUp' });
  expect(screen.getByRole('option', { name: 'beta' })).toHaveAttribute('aria-selected', 'true');
  fireEvent.mouseDown(screen.getByRole('option', { name: 'alpha' }));
  expect(props.onSubmitBulkAddTag).toHaveBeenCalledWith('alpha');
  expect(input).toHaveFocus();
});

it('uses the controlled draft when the apply button is clicked', () => {
  const { props } = renderToolbar({ addTagMenuOpen: true, tagDraft: 'typed' });
  fireEvent.click(screen.getByRole('button', { name: 'Übernehmen' }));
  expect(props.onSubmitBulkAddTag).toHaveBeenCalledWith();
});

it('sets a bulk license, clears it, and closes its menu on Escape', async () => {
  const onBulkSetLicense = vi.fn().mockResolvedValue(undefined);
  const { props } = renderToolbar({ onBulkSetLicense });
  clickBulkMore('Lizenz setzen');
  chooseSelect(screen.getByRole('combobox', { name: 'Lizenz' }), 'personal');
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  await waitFor(() => expect(onBulkSetLicense).toHaveBeenCalledWith('personal'));
  await waitFor(() => expect(screen.queryByRole('combobox')).toBeNull());
  clickBulkMore('Lizenz setzen');
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  await waitFor(() => expect(onBulkSetLicense).toHaveBeenLastCalledWith(null));
  await waitFor(() => expect(screen.queryByRole('combobox')).toBeNull());
  clickBulkMore('Lizenz setzen');
  fireEvent.keyDown(window, { key: 'Escape' });
  expect(screen.queryByRole('combobox')).toBeNull();
  expect(props.onClearSelection).not.toHaveBeenCalled();
  expect(screen.getByRole('button', { name: 'Ablegen' })).toHaveFocus();
});

it('applies a custom license to the selection and reports errors without closing', async () => {
  const onBulkSetLicense = vi.fn().mockRejectedValueOnce(new Error('Disk full')).mockResolvedValue(undefined);
  renderToolbar({ onBulkSetLicense });
  clickBulkMore('Lizenz setzen');
  chooseSelect(screen.getByRole('combobox'), '__other');
  fireEvent.change(screen.getByRole('textbox', { name: 'Lizenztext' }), { target: { value: ' Agreement ' } });
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('Disk full');
  expect(screen.getByRole('textbox')).toHaveValue(' Agreement ');
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  await waitFor(() => expect(screen.queryByRole('combobox')).toBeNull());
  expect(onBulkSetLicense).toHaveBeenLastCalledWith(' Agreement ');
});


describe.each(['tags', 'collections'] as const)('%s list menu', kind => {
  function openList(count: number) {
    const names = Array.from({ length: count }, (_, i) => `entry-${i}`);
    const menu = kind === 'tags' ? 'removeTagMenuOpen' : 'addToCollectionMenuOpen';
    const onChange = kind === 'tags' ? 'onRemoveTagMenuOpenChange' : 'onAddToCollectionMenuOpenChange';
    const rendered = renderToolbar({
      [menu]: true,
      tagsInSelection: names,
      collections: names.map(name => ({ id: name, name, modelCount: 0 })),
    });
    const entries = names.map(name => screen.getByRole('button', { name: kind === 'tags' ? `#${name}` : name }));
    return { ...rendered, entries, menu, onChange } as const;
  }

  it.each([3, 40])('bounds and contains scrolling for %s entries', count => {
    const { entries } = openList(count);
    expect(entries[0].parentElement).toHaveClass('max-h-[max(11rem,60vh)]', 'overflow-y-auto', 'overscroll-contain');
  });

  it('moves focus with arrows, scrolls the entry into view and stops at the edges', () => {
    const { entries } = openList(3);
    const scrolls = entries.map(entry => {
      const scroll = vi.fn();
      Object.defineProperty(entry, 'scrollIntoView', { configurable: true, value: scroll });
      return scroll;
    });
    entries[0].focus();
    for (const [key, expected] of [['ArrowUp', 0], ['ArrowDown', 1], ['ArrowDown', 2], ['ArrowDown', 2], ['ArrowUp', 1], ['ArrowUp', 0]] as const) {
      fireEvent.keyDown(document.activeElement!, { key });
      expect(document.activeElement).toBe(entries[expected]);
      expect(scrolls[expected]).toHaveBeenCalledWith({ block: 'nearest' });
    }
  });

  it('retains Escape closing and focus restoration after arrow navigation', () => {
    const { props, view, entries, menu, onChange } = openList(3);
    entries[0].focus();
    fireEvent.keyDown(entries[0], { key: 'ArrowDown' });
    expect(entries[1]).toHaveFocus();
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(props[onChange]).toHaveBeenCalledWith(false);
    view.rerender(<LanguageProvider><BulkActionToolbar {...props} {...{ [menu]: false }} /></LanguageProvider>);
    expect(screen.queryByRole('button', { name: kind === 'tags' ? '#entry-0' : 'entry-0' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Ablegen' })).toHaveFocus();
    expect(props.onClearSelection).not.toHaveBeenCalled();
  });
});

describe('popover anchoring', () => {
  it.each([
    ['addTagMenuOpen', '.absolute:not([role="menu"])'],
    ['addToCollectionMenuOpen', '[data-collection-popover]'],
    ['removeTagMenuOpen', '[data-remove-tag-popover]'],
  ] as const)('anchors the %s popover to the organize group button', (menu, selector) => {
    renderToolbar({ [menu]: true });
    const organize = screen.getByRole('button', { name: de.bulkGroupFile });
    const wrapper = organize.parentElement!;
    const popover = wrapper.querySelector(selector);
    expect(popover).not.toBeNull();
    expect(screen.getByRole('button', { name: de.bulkGroupMark }).parentElement).not.toBe(wrapper);
    expect(screen.getByRole('button', { name: de.bulkGroupMark }).parentElement!.querySelector('[data-navigation-menu]')).toBeNull();
  });

  it('anchors the license popover to the organize group button', () => {
    renderToolbar({ onBulkSetLicense: vi.fn(async () => {}) });
    openBulkMore();
    fireEvent.click(screen.getByRole('menuitem', { name: de.licenseSet }));
    expect(screen.getByRole('button', { name: de.bulkGroupFile }).parentElement!.querySelector('[data-license-popover]')).not.toBeNull();
  });
});

describe('BulkActionToolbar move', () => {
  it('has a "Verschieben …" button that opens the move dialog', () => {
    const onBulkMove = vi.fn();
    renderToolbar({ onBulkMove });
    clickBulkMore('Verschieben nach …');
    expect(onBulkMove).toHaveBeenCalledTimes(1);
  });

  it('shows no move button when the caller gives no handler', () => {
    renderToolbar();
    expect(screen.queryByRole('button', { name: 'Verschieben …' })).toBeNull();
  });
});

it('disables the move button while an import runs', () => {
  render(<LanguageProvider><ImportLockContext.Provider value={true}>
    <BulkActionToolbar {...{
      selectedCount: 2, confirmBulkDelete: false, onConfirmBulkDeleteChange: vi.fn(), onSelectAllVisible: vi.fn(), onClearSelection: vi.fn(),
      onBulkAddToQueue: vi.fn(), addToCollectionMenuOpen: false, onAddToCollectionMenuOpenChange: vi.fn(), collections: [], onBulkAddToCollection: vi.fn(),
      activeCollection: null, onBulkRemoveFromCollection: vi.fn(), onBulkSetPrintStatus: vi.fn(), onBulkDelete: vi.fn(), onBulkMove: vi.fn(), addTagMenuOpen: false,
      onAddTagMenuOpenChange: vi.fn(), allTags: [], tagsInAllSelection: [], tagDraft: '', onTagDraftChange: vi.fn(), onSubmitBulkAddTag: vi.fn(),
      removeTagMenuOpen: false, onRemoveTagMenuOpenChange: vi.fn(), tagsInSelection: [], onBulkRemoveTag: vi.fn(),
    }} />
  </ImportLockContext.Provider></LanguageProvider>);
  expect(within(openBulkMore()).getByRole('menuitem', { name: 'Verschieben nach …' })).toBeDisabled();
});

describe('BulkActionToolbar without a selection', () => {
  const sampleProps = { onBulkMove: vi.fn(), onBulkSetLicense: vi.fn(async () => {}), selectedIds: ['1'], onDateRenamed: vi.fn() };

  it('is shown with "Alle auswählen" usable and every bulk action unavailable with a reason', () => {
    const { props } = renderToolbar({ selectedCount: 0, ...sampleProps });
    const bar = screen.getByRole('toolbar', { name: 'Aktionen für die Auswahl' });
    expect(within(bar).getByText('0 ausgewählt')).toBeInTheDocument();
    const selectAll = within(bar).getByRole('button', { name: 'Alle auswählen' });
    expect(selectAll).not.toHaveAttribute('aria-disabled');
    fireEvent.click(selectAll);
    expect(props.onSelectAllVisible).toHaveBeenCalledTimes(1);

    const unavailable = ['Auswahl aufheben', 'Markieren', 'Ablegen', 'Bündel', 'In den Papierkorb'];
    for (const name of unavailable) {
      const button = within(bar).getByRole('button', { name });
      // aria-disabled instead of disabled keeps the button focusable, so its reason can be read.
      expect(button).toHaveAttribute('aria-disabled', 'true');
      expect(button).toHaveAccessibleDescription('Wähle zuerst Modelle aus.');
      fireEvent.click(button);
    }
    expect(props.onClearSelection).not.toHaveBeenCalled();
    expect(props.onBulkAddToQueue).not.toHaveBeenCalled();
    expect(props.onBulkSetPrintStatus).not.toHaveBeenCalled();
    expect(props.onConfirmBulkDeleteChange).not.toHaveBeenCalled();
    expect(props.onAddTagMenuOpenChange).not.toHaveBeenCalled();
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('has the same classes with and without a selection, so the view does not jump', () => {
    const { view } = renderToolbar({ selectedCount: 0 });
    const empty = screen.getByRole('toolbar').className;
    view.unmount();
    renderToolbar({ selectedCount: 3 });
    expect(screen.getByRole('toolbar').className).toBe(empty);
    expect(empty).toContain('min-h-12');
  });

  it('does not clear anything with Escape', () => {
    const { props } = renderToolbar({ selectedCount: 0 });
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onClearSelection).not.toHaveBeenCalled();
  });

  it('keeps focus on "Auswahl aufheben" when it becomes unavailable after use', () => {
    const { props, view } = renderToolbar({ selectedCount: 2 });
    const clear = screen.getByRole('button', { name: 'Auswahl aufheben' });
    clear.focus();
    fireEvent.click(clear);
    view.rerender(<LanguageProvider><BulkActionToolbar {...props} selectedCount={0} /></LanguageProvider>);
    expect(screen.getByRole('button', { name: 'Auswahl aufheben' })).toHaveFocus();
  });

  it('closes an open menu and confirmation when the selection disappears', () => {
    const { props, view } = renderToolbar({ selectedCount: 2, confirmBulkDelete: true, addTagMenuOpen: true });
    view.rerender(<LanguageProvider><BulkActionToolbar {...props} selectedCount={0} /></LanguageProvider>);
    expect(props.onConfirmBulkDeleteChange).toHaveBeenCalledWith(false);
    expect(props.onAddTagMenuOpenChange).toHaveBeenCalledWith(false);
  });
});

describe('BulkActionToolbar action groups', () => {
  const extra = { onBulkMove: vi.fn(), onBulkSetLicense: vi.fn(async () => {}), selectedIds: ['1'], onDateRenamed: vi.fn(), activeCollection: 'c1' };
  const names = (scope: HTMLElement, role: 'button' | 'menuitem') => within(scope).getAllByRole(role).map(el => el.textContent?.trim());

  it('keeps selection, three group menus and trash in a stable order', () => {
    renderToolbar(extra);
    const bar = screen.getByRole('toolbar');
    expect(names(bar, 'button')).toEqual(['Alle auswählen', 'Auswahl aufheben', 'Markieren', 'Ablegen', 'Bündel', 'In den Papierkorb']);
    expect(screen.getByText('2 ausgewählt')).toHaveAttribute('aria-live', 'polite');
  });

  it('uses the common organize labels and ordering, then preserves auxiliary actions', () => {
    renderToolbar(extra);
    const trigger = screen.getByRole('button', { name: 'Ablegen' });
    expect(trigger).toHaveAttribute('aria-haspopup', 'menu');
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    const menu = openBulkMore();
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(names(menu, 'menuitem')).toEqual(['Tag hinzufügen', 'Tag entfernen', 'Zu Sammlung hinzufügen', 'Aus Sammlung entfernen', 'Lizenz setzen', 'Verschieben nach …', 'Aus Katalog entfernen', 'Mit Datum umbenennen …', 'Druckdaten nachlesen']);
  });

  it('lists "Aus Sammlung entfernen" only inside a collection', () => {
    renderToolbar({ ...extra, activeCollection: null });
    expect(names(openBulkMore(), 'menuitem')).not.toContain('Aus Sammlung entfernen');
  });

  it('opens with ArrowDown, moves with the arrow keys, closes with Escape and returns focus to the trigger', () => {
    const { props } = renderToolbar(extra);
    const trigger = screen.getByRole('button', { name: 'Ablegen' });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: 'ArrowDown' });
    const items = within(screen.getByRole('menu')).getAllByRole('menuitem');
    expect(items[0]).toHaveFocus();
    fireEvent.keyDown(items[0], { key: 'ArrowDown' });
    expect(items[1]).toHaveFocus();
    fireEvent.keyDown(items[1], { key: 'End' });
    expect(items[items.length - 1]).toHaveFocus();
    fireEvent.keyDown(items[items.length - 1], { key: 'ArrowUp' });
    expect(items[items.length - 2]).toHaveFocus();
    fireEvent.keyDown(items[1], { key: 'Home' });
    expect(items[0]).toHaveFocus();
    fireEvent.keyDown(items[0], { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(trigger).toHaveFocus();
    expect(props.onClearSelection).not.toHaveBeenCalled();
  });

  it('closes on Tab and on a click outside', () => {
    renderToolbar(extra);
    const trigger = screen.getByRole('button', { name: 'Ablegen' });
    openBulkMore();
    fireEvent.keyDown(document.activeElement!, { key: 'Tab' });
    expect(screen.queryByRole('menu')).toBeNull();
    openBulkMore();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole('menu')).toBeNull();
    expect(trigger).toBeInTheDocument();
  });

  it('runs a menu action, closes the menu and keeps focus in the bar', () => {
    const { props } = renderToolbar(extra);
    clickBulkMore('Als gedruckt markieren');
    expect(props.onBulkSetPrintStatus).toHaveBeenCalledWith('printed');
    expect(screen.queryByRole('menu')).toBeNull();
    expect(screen.getByRole('button', { name: 'Markieren' })).toHaveFocus();
    fireEvent.click(within(openBulkMore()).getByRole('menuitem', { name: 'Aus Sammlung entfernen' }));
    expect(props.onBulkRemoveFromCollection).toHaveBeenCalledTimes(1);
  });

  const formerActions = ['Alle auswählen', 'Auswahl aufheben', 'Lizenz setzen', 'Zur Warteschlange hinzufügen', 'Zu Sammlung hinzufügen',
    'Aus Sammlung entfernen', 'Tag hinzufügen', 'Tag entfernen', 'Als gedruckt markieren', 'Mit Datum umbenennen …', 'Verschieben nach …',
    'Aus Katalog entfernen', 'In den Papierkorb'];
  it.each([500, 1400])('keeps actions in the same groups at width %s', width => {
    window.innerWidth = width;
    renderToolbar(extra);
    const inBar = names(screen.getByRole('toolbar'), 'button');
    const inMenu = ['Markieren', 'Ablegen'].flatMap(group => within(openBulkMore(group)).queryAllByRole('menuitem').map(el => el.textContent?.trim()));
    for (const action of formerActions) expect([...inBar, ...inMenu], action).toContain(action);
  });

  it('keeps tags and move inside Ablegen and printed inside Markieren', () => {
    const { props } = renderToolbar(extra);
    const bar = screen.getByRole('toolbar');
    expect(within(bar).queryByRole('button', { name: 'Als gedruckt markieren' })).toBeNull();
    expect(within(bar).queryByRole('button', { name: 'Verschieben nach …' })).toBeNull();
    clickBulkMore('Tag entfernen');
    expect(props.onRemoveTagMenuOpenChange).toHaveBeenCalledWith(true);
  });
});

describe('BulkActionToolbar focus handover from a group menu', () => {
  it('moves focus into the license editor and back to "Ablegen" on Escape', () => {
    renderToolbar({ onBulkSetLicense: vi.fn().mockResolvedValue(undefined) });
    clickBulkMore('Lizenz setzen');
    const select = screen.getByRole('combobox', { name: 'Lizenz' });
    expect(select).toHaveFocus();
    fireEvent.keyDown(select, { key: 'ArrowDown' });
    expect(screen.getByRole('listbox')).toBeVisible();
    fireEvent.keyDown(select, { key: 'Escape' });
    expect(screen.queryByRole('listbox')).toBeNull();
    expect(select).toHaveFocus();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('combobox', { name: 'Lizenz' })).toBeNull();
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  });

  it('moves focus into the tag list in the group menu and back to "Ablegen" on Escape', () => {
    const view = renderToolbar({});
    clickBulkMore('Tag entfernen');
    expect(view.props.onRemoveTagMenuOpenChange).toHaveBeenCalledWith(true);
    view.view.rerender(<LanguageProvider><BulkActionToolbar {...view.props} removeTagMenuOpen /></LanguageProvider>);
    expect(screen.getByRole('button', { name: '#miniatur' })).toHaveFocus();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  });

  it('focuses the empty tag list itself so Escape still works from there', () => {
    renderToolbar({ removeTagMenuOpen: true, tagsInSelection: [] });
    expect(document.querySelector('[data-remove-tag-popover]')).toHaveFocus();
  });

  it('returns focus to "Ablegen" after a tag was picked and the list closed', () => {
    const view = renderToolbar({ removeTagMenuOpen: true });
    const tag = screen.getByRole('button', { name: '#miniatur' });
    expect(tag).toHaveFocus();
    view.view.rerender(<LanguageProvider><BulkActionToolbar {...view.props} removeTagMenuOpen={false} /></LanguageProvider>);
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  });

  it('keeps the focus return to "Ablegen" after the date rename dialog is cancelled', () => {
    renderToolbar({ selectedIds: ['a', 'b'], onDateRenamed: vi.fn() });
    clickBulkMore('Mit Datum umbenennen …');
    expect(screen.getByRole('dialog')).toContainElement(document.activeElement as HTMLElement);
    fireEvent.keyDown(document.activeElement ?? window, { key: 'Escape' });
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  });

  it('moves focus to the confirmation Cancel button and back to "Ablegen" for "Aus Katalog entfernen"', () => {
    renderToolbar();
    clickBulkMore('Aus Katalog entfernen');
    const cancel = screen.getByRole('button', { name: 'Abbrechen' });
    expect(cancel).toHaveFocus();
    fireEvent.click(cancel);
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  });

  it('returns focus to "Ablegen" after "Aus Sammlung entfernen"', () => {
    renderToolbar({ activeCollection: 'c1' });
    clickBulkMore('Aus Sammlung entfernen');
    expect(within(screen.getByRole('toolbar')).getByRole('button', { name: 'Ablegen' })).toHaveFocus();
  });
});

it.each(['de', 'en'] as const)('renders shared action labels in shared order (%s)', language => {
  localStorage.setItem('3mf-katalog-language', language);
  renderToolbar({ onBulkMove: vi.fn(), onBulkSetLicense: vi.fn(), activeCollection: 'c',
    bundleActions: { canCreate: true, canAddTo: true, canRemove: true, canEdit: false, canDissolve: false,
      onCreate: vi.fn(), onAddTo: vi.fn(), onRemove: vi.fn(), onEdit: vi.fn(), onDissolve: vi.fn() } });
  const translations = language === 'de' ? de : en;
  const actions = modelMenuActions({ web: false, count: 2, favorite: false, queue: false, printed: false });
  for (const [group, label] of [['quick', translations.bulkGroupMark], ['organize', translations.bulkGroupFile], ['bundle', translations.modelMenuBundle]] as const) {
    const menu = openBulkMore(label);
    const rendered = within(menu).getAllByRole('menuitem').map(item => item.textContent);
    const expected = actions.filter(action => action.group === group).map(action => translations[action.label]);
    expect(rendered.slice(0, expected.length)).toEqual(expected);
  }
});

it('locks file changes, license and re-reading during import, keeping tags and collections available', () => {
  const { props, view } = renderToolbar({ onBulkMove: vi.fn(), onBulkSetLicense: vi.fn(), selectedIds: ['a', 'b'], onDateRenamed: vi.fn() });
  view.rerender(<LanguageProvider><ImportLockContext.Provider value={true}><BulkActionToolbar {...props} /></ImportLockContext.Provider></LanguageProvider>);
  for (const name of ['Verschieben nach …', 'Aus Katalog entfernen', 'Mit Datum umbenennen …', 'Lizenz setzen', 'Druckdaten nachlesen']) {
    expect(within(openBulkMore()).getByRole('menuitem', { name })).toBeDisabled();
  }
  for (const name of ['Tag hinzufügen', 'Zu Sammlung hinzufügen']) {
    expect(within(openBulkMore()).getByRole('menuitem', { name })).not.toBeDisabled();
  }
  expect(screen.getByRole('button', { name: 'In den Papierkorb' })).toBeDisabled();
  for (const item of within(openBulkMore('Markieren')).getAllByRole('menuitem')) expect(item).not.toBeDisabled();
});

it('anchors each group menu at its own trigger', () => {
  renderToolbar();
  for (const name of ['Markieren', 'Ablegen']) {
    const menu = openBulkMore(name);
    expect(menu.parentElement).toBe(screen.getByRole('button', { name }).parentElement);
    expect(menu.className).toContain('left-0');
    expect(menu.className).not.toContain('right-0');
  }
});

it('toggles all-marked states back through the bulk handlers', () => {
  const { props } = renderToolbar({ favorite: true, queue: true, printed: true });
  clickBulkMore(de.favoriteRemove);
  clickBulkMore(de.removeFromQueue);
  clickBulkMore(de.markAsNotPrinted);
  expect(props.onBulkToggleFavorite).toHaveBeenCalledOnce();
  expect(props.onBulkRemoveFromQueue).toHaveBeenCalledOnce();
  expect(props.onBulkSetPrintStatus).toHaveBeenCalledExactlyOnceWith('not_printed');
});
