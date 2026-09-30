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
  it('closes an open menu with Escape instead of leaving it under the next click', () => {
    const { props } = renderToolbar({ removeTagMenuOpen: true });
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onRemoveTagMenuOpenChange).toHaveBeenCalledWith(false);
    expect(props.onBulkRemoveTag).not.toHaveBeenCalled();
  });

  it('does not react to Escape while no menu is open', () => {
    const { props } = renderToolbar();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(props.onRemoveTagMenuOpenChange).not.toHaveBeenCalled();
    expect(props.onAddTagMenuOpenChange).not.toHaveBeenCalled();
    expect(props.onAddToCollectionMenuOpenChange).not.toHaveBeenCalled();
  });

  it('wraps onto further rows so "remove from catalog" and "delete" stay reachable in narrow windows', () => {
    renderToolbar();
    const remove = screen.getByRole('button', { name: 'Aus Katalog entfernen' });
    const bar = remove.closest('div.flex-none');
    expect(bar?.className).toContain('flex-wrap');
  });
});
