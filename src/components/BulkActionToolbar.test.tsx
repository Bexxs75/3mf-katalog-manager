import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { BulkActionToolbar } from './BulkActionToolbar';

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

  it('keeps the selection during bulk delete confirmation', () => {
    const { props } = renderToolbar({ confirmBulkDelete: true });
    fireEvent.keyDown(window, { key: 'Escape' });
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

  it('closes the tag input menu without clearing the selection', () => {
    const { props } = renderToolbar({ addTagMenuOpen: true });
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Escape' });
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
