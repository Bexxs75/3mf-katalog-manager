import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { BulkActionToolbar } from './BulkActionToolbar';
import { ImportLockContext } from '../hooks/ImportLockContext';

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
    fireEvent.click(screen.getByRole('button', { name: 'Aus Katalog entfernen' }));
    fireEvent.keyDown(screen.getByRole('button', { name: 'Abbrechen' }), { key: 'Escape' });
    expect(screen.getByRole('button', { name: 'Aus Katalog entfernen' })).toBeInTheDocument();
    expect(props.onClearSelection).not.toHaveBeenCalled();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onClearSelection).toHaveBeenCalledTimes(1);
  });

  it('cancels remove confirmation with focus outside the bar before clearing selection', () => {
    const { props } = renderToolbar();
    fireEvent.click(screen.getByRole('button', { name: 'Aus Katalog entfernen' }));
    const outside = document.createElement('button');
    document.body.appendChild(outside);
    try {
      outside.focus();
      expect(outside).toHaveFocus();
      fireEvent.keyDown(outside, { key: 'Escape' });
      expect(screen.getByRole('button', { name: 'Aus Katalog entfernen' })).toBeInTheDocument();
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
      fireEvent.click(screen.getByRole('button', { name: 'Aus Katalog entfernen' }));
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

  it('wraps onto further rows so "remove from catalog" and "delete" stay reachable in narrow windows', () => {
    renderToolbar();
    const remove = screen.getByRole('button', { name: 'Aus Katalog entfernen' });
    const bar = remove.closest('div.flex-none');
    expect(bar?.className).toContain('flex-wrap');
  });
});

it('focuses Cancel in the inline remove confirmation and restores the remove button on Escape', () => {
  const { props } = renderToolbar();
  const trigger = screen.getByRole('button', { name: 'Aus Katalog entfernen' });
  trigger.focus(); fireEvent.click(trigger);
  expect(screen.getByRole('button', { name: 'Abbrechen' })).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
  expect(screen.getByRole('button', { name: 'Aus Katalog entfernen' })).toHaveFocus();
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
  expect(screen.getByRole('button', { name: 'Tag hinzufügen' })).toHaveFocus();
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
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz setzen' }));
  fireEvent.change(screen.getByRole('combobox', { name: 'Lizenz' }), { target: { value: 'personal' } });
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  await waitFor(() => expect(onBulkSetLicense).toHaveBeenCalledWith('personal'));
  await waitFor(() => expect(screen.queryByRole('combobox')).toBeNull());
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz setzen' }));
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  await waitFor(() => expect(onBulkSetLicense).toHaveBeenLastCalledWith(null));
  await waitFor(() => expect(screen.queryByRole('combobox')).toBeNull());
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz setzen' }));
  fireEvent.keyDown(window, { key: 'Escape' });
  expect(screen.queryByRole('combobox')).toBeNull();
  expect(props.onClearSelection).not.toHaveBeenCalled();
  expect(screen.getByRole('button', { name: 'Lizenz setzen' })).toHaveFocus();
});

it('applies a custom license to the selection and reports errors without closing', async () => {
  const onBulkSetLicense = vi.fn().mockRejectedValueOnce(new Error('Disk full')).mockResolvedValue(undefined);
  renderToolbar({ onBulkSetLicense });
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz setzen' }));
  fireEvent.change(screen.getByRole('combobox'), { target: { value: '__other' } });
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
    expect(screen.getByRole('button', { name: kind === 'tags' ? 'Tag entfernen' : 'Zu Sammlung hinzufügen' })).toHaveFocus();
    expect(props.onClearSelection).not.toHaveBeenCalled();
  });
});

describe('BulkActionToolbar move', () => {
  it('has a "Verschieben …" button that opens the move dialog', () => {
    const onBulkMove = vi.fn();
    renderToolbar({ onBulkMove });
    fireEvent.click(screen.getByRole('button', { name: 'Verschieben …' }));
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
  expect(screen.getByRole('button', { name: 'Verschieben …' })).toBeDisabled();
});
