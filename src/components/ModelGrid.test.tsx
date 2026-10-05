import { beforeEach, expect, it, vi } from 'vitest';
import { act, render } from '@testing-library/react';
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

it.each(['compact', 'comfort'])('reserves equal metadata and tag heights for 0, 2 and 12 tags (%s)', density => {
  localStorage.setItem('3mf-katalog-density', density);
  const models = [0, 2, 12].map((count, i) => makeModelFile({ id: String(i), tags: Array.from({ length: count }, (_, j) => `Tag ${j}`) }));
  const { container } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} models={models} /></UiDensityProvider></LanguageProvider>);
  const metadata = [...container.querySelectorAll<HTMLElement>('[data-card-metadata]')];
  expect(metadata).toHaveLength(3);
  expect(new Set(metadata.map(node => node.style.height)).size).toBe(1);
  expect(metadata[0].style.height).not.toBe('');
  expect(container.querySelectorAll('[data-card-tags]')).toHaveLength(3);
});

it.each(['compact', 'comfort'])('counts overflow including space for the counter and remeasures on resize (%s)', density => {
  localStorage.setItem('3mf-katalog-density', density);
  const callbacks = new Set<() => void>();
  vi.stubGlobal('ResizeObserver', class {
    constructor(private callback: () => void) { callbacks.add(callback); }
    observe() {} disconnect() { callbacks.delete(this.callback); }
  });
  let width = 140;
  const widthSpy = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (this: HTMLElement) { return this.hasAttribute('data-card-tags') ? width : 800; });
  const leftSpy = vi.spyOn(HTMLElement.prototype, 'offsetLeft', 'get').mockImplementation(function (this: HTMLElement) { return Number(this.dataset.tagIndex ?? 0) * 54; });
  const chipSpy = vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockImplementation(function (this: HTMLElement) { return this.hasAttribute('data-tag-counter-probe') ? 30 : 50; });
  try {
    const tags = ['Alpha', 'Beta', 'Gamma', 'Delta'];
    const { container, rerender } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} models={[makeModelFile({ tags })]} /></UiDensityProvider></LanguageProvider>);
    const counter = container.querySelector('[data-more-tags]');
    expect(counter).toHaveTextContent('+2');
    tags.forEach(tag => expect(counter?.getAttribute('title')).toContain(tag));
    expect(counter?.getAttribute('title')).toContain('2 weitere Tags');
    act(() => { width = 100; callbacks.forEach(callback => callback()); });
    expect(container.querySelector('[data-more-tags]')).toHaveTextContent('+3');
    act(() => { width = 212; callbacks.forEach(callback => callback()); });
    expect(container.querySelector('[data-more-tags]')).toBeNull();
    rerender(<LanguageProvider><UiDensityProvider><ModelGrid {...props} models={[makeModelFile({ tags: ['Replacement'] })]} /></UiDensityProvider></LanguageProvider>);
    expect(container.querySelector('[data-more-tags]')).toBeNull();
  } finally {
    widthSpy.mockRestore(); leftSpy.mockRestore(); chipSpy.mockRestore(); vi.unstubAllGlobals();
  }
});

it('keeps the grid hidden until a positive local width has been measured', () => {
  let resize!: () => void;
  const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 0 } as DOMRect);
  vi.stubGlobal('ResizeObserver', class {
    constructor(callback: () => void) { resize = callback; }
    observe() {} disconnect() {}
  });
  const props = { models: [makeModelFile({tags: []})], selectedId: null, onSelect: vi.fn(), onOpenDetail: vi.fn(),
    onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(), onToggleBulkSelect: vi.fn(),
    displayPreference: 'thumbnail' as const };
  const { container } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props} /></UiDensityProvider></LanguageProvider>);
  expect(container.querySelector('.grid.gap-3\\.5')?.parentElement).toHaveStyle({ visibility: 'hidden' });
  rect.mockReturnValue({ width: 800, height: 100, top: 0 } as DOMRect);
  act(() => resize());
  expect(container.querySelector('.grid.gap-3\\.5')).toHaveStyle({gridTemplateColumns: 'repeat(4, minmax(0, 1fr))'});
});

it.each(['compact', 'comfort'])('disables native thumbnail dragging in both card layouts (%s)', density => {
  localStorage.setItem('3mf-katalog-density', density);
  const { container } = render(<LanguageProvider><UiDensityProvider><ModelGrid {...props}
    models={[makeModelFile({ thumbnailImage: 'data:image/png;base64,eA==' })]} /></UiDensityProvider></LanguageProvider>);
  expect(container.querySelector('[data-model-id] img')).toHaveAttribute('draggable', 'false');
});
