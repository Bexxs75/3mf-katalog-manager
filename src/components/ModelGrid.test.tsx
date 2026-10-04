import { beforeEach, expect, it, vi } from 'vitest';
import { render } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
beforeEach(() => localStorage.clear());
const props = { models: [makeModelFile()], selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(), onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn(), displayPreference: 'thumbnail' as const };
it.each(['compact', 'comfort', 'list'])('shows a grip only for movable models (%s)', (view) => {
  localStorage.setItem('3mf-katalog-density', view);
  const Component = view === 'list' ? ModelList : ModelGrid;
  const ui = (extra = {}) => <LanguageProvider><UiDensityProvider><Component {...props} {...extra} /></UiDensityProvider></LanguageProvider>;
  const { container, rerender } = render(ui());
  expect(container.querySelector('[data-drag-grip]')).toBeNull();
  rerender(ui({ onDragFileStart: vi.fn() }));
  expect(container.querySelector('[data-drag-grip]')).toHaveAttribute('aria-hidden', 'true');
  rerender(ui({ onDragFileStart: vi.fn(), readOnly: true }));
  expect(container.querySelector('[data-drag-grip]')).toBeNull();
  if (view !== 'list') {
    rerender(ui({ onDragFileStart: vi.fn(), reorderable: true }));
    expect(container.querySelector('[data-drag-grip]')).toBeNull();
  }
});
