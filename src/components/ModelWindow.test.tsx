import { act, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useContext, useRef, type ComponentProps } from 'react';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { GroupedModelGrid } from './GroupedModelGrid';
import { GroupedModelList } from './GroupedModelList';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ModelLayoutContext, ModelLayoutProvider } from '../hooks/ModelLayoutContext';
import { scrollTileIntoView } from '../hooks/useKeyboardShortcuts';
import { makeModelFile } from '../test/factories';
vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn().mockResolvedValue([]) }));
let width = 800;
const resizeCallbacks = new Set<() => void>();
beforeEach(() => {
  localStorage.clear(); width = 800;
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(() => width);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(400);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    const top = this.dataset.catalogScroller !== undefined ? 0 : -(this.closest<HTMLElement>('[data-catalog-scroller]')?.scrollTop ?? 0);
    return { top, left: 0, width, height: this.dataset.modelId ? 100 : 0, right: width, bottom: 100 } as DOMRect;
  });
  vi.stubGlobal('ResizeObserver', class {
    constructor(private callback: () => void) { resizeCallbacks.add(callback); }
    observe() {} disconnect() { resizeCallbacks.delete(this.callback); }
  });
});
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); resizeCallbacks.clear(); });
const models = Array.from({ length: 200 }, (_, i) => makeModelFile({ id: String(i), name: `Model ${i}`, importedAt: new Date().toISOString() }));
const actions = { selectedId: '0', onSelect: vi.fn(), onOpenDetail: vi.fn(), onContextMenu: vi.fn(), onToggleFavorite: vi.fn(), selectedForBulk: new Set<string>(['0']), onToggleBulkSelect: vi.fn(), displayPreference: 'thumbnail' as const };
function ScrollTrigger() {
  const registry = useContext(ModelLayoutContext);
  return <button onClick={() => {
    const layout = [...(registry?.layouts.values() ?? [])].find(layout => layout.order.includes('120'));
    scrollTileIntoView('120', layout);
  }}>Jump</button>;
}
function Harness({ list = false, reorderable = false, grouped = false, count = 200 }: { list?: boolean; reorderable?: boolean; grouped?: boolean; count?: number }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const groupProps = { folders: [], draggedFolderId: null, dragOverFolderId: null, onDragFolderStart: vi.fn(), onFolderMouseEnter: vi.fn(), onFolderMouseLeave: vi.fn(), collapsedFolders: { isCollapsed: () => false, toggle: vi.fn() } } as unknown as Omit<ComponentProps<typeof GroupedModelGrid>, keyof typeof actions | 'models' | 'containerRef'>;
  const content = grouped ? list
    ? <GroupedModelList {...actions} {...groupProps} models={models.slice(0, count)} containerRef={containerRef} />
    : <GroupedModelGrid {...actions} {...groupProps} models={models.slice(0, count)} containerRef={containerRef} />
    : list ? <ModelList {...actions} models={models} containerRef={containerRef} />
    : <ModelGrid {...actions} models={models} containerRef={containerRef} reorderable={reorderable} />;
  return <LanguageProvider><UiDensityProvider><ModelLayoutProvider><ScrollTrigger /><div ref={containerRef} data-catalog-scroller>{content}</div></ModelLayoutProvider></UiDensityProvider></LanguageProvider>;
}
it('windows aligned grid rows, keeps selection and badges, and remeasures columns', async () => {
  const { container } = render(<Harness />);
  await waitFor(() => expect(container.querySelectorAll('[data-model-id]').length).toBe(24));
  expect(container.querySelector('[data-model-id="0"]')?.className).toContain('border-[var(--accent)]');
  expect(container.querySelector('[data-model-id="0"] input')).toBeChecked();
  expect(container.querySelector('[data-model-id="0"]')?.textContent).toContain('NEU');
  expect(container.querySelector<HTMLElement>('[data-window-spacer="bottom"]')?.style.height).toBe('5016px');
  fireEvent.click(container.querySelector('[data-model-id="0"]')!);
  expect(actions.onSelect).toHaveBeenCalledWith('0');
  const scroller = container.querySelector<HTMLElement>('[data-catalog-scroller]')!;
  await act(async () => { scroller.scrollTop = 1140; scroller.dispatchEvent(new Event('scroll')); await new Promise(resolve => requestAnimationFrame(resolve)); });
  expect(container.querySelector('[data-model-id="0"]')).toBeNull();
  expect(container.querySelector<HTMLElement>('[data-window-spacer="top"]')?.style.height).toBe('912px');
  act(() => { width = 400; resizeCallbacks.forEach(callback => callback()); });
  expect(container.querySelector('.grid.gap-3\\.5')?.getAttribute('style')).toContain('repeat(2,');
});
it('leaves reorderable grids complete', () => {
  const { container } = render(<Harness reorderable />);
  expect(container.querySelectorAll('[data-model-id]')).toHaveLength(200);
});
it('windows list rows and measures row height', async () => {
  const { container } = render(<Harness list />);
  await waitFor(() => expect(container.querySelectorAll('[data-model-id]')).toHaveLength(6));
  expect(container.querySelector<HTMLElement>('[data-window-spacer="bottom"]')?.style.height).toBe('19400px');
});
it.each([false, true])('windows only groups above 60 models (list=%s)', list => {
  const { container, rerender } = render(<Harness grouped list={list} count={60} />);
  expect(container.querySelectorAll('[data-model-id]')).toHaveLength(60);
  rerender(<Harness grouped list={list} count={61} />);
  expect(container.querySelectorAll('[data-model-id]').length).toBeLessThan(61);
});

it('scrolls an offscreen card into view after the next rendered window', async () => {
  const scroll = vi.fn();
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: scroll });
  const { container, getByText } = render(<Harness />);
  await waitFor(() => expect(container.querySelectorAll('[data-model-id]')).toHaveLength(24));
  expect(container.querySelector('[data-model-id="120"]')).toBeNull();
  fireEvent.click(getByText('Jump'));
  await waitFor(() => expect(container.querySelector('[data-model-id="120"]')).toBeInTheDocument());
  await waitFor(() => expect(scroll).toHaveBeenCalledWith({ block: 'nearest' }));
  delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
});

it.each(['compact', 'comfort'])('accounts exactly for all measured rows and gaps (%s)', density => {
  localStorage.setItem('3mf-katalog-density', density);
  const { container } = render(<Harness />);
  const cards = container.querySelectorAll('[data-model-id]');
  const columns = density === 'comfort' ? 3 : 4;
  const totalRows = Math.ceil(models.length / columns);
  const visibleRows = Math.ceil(cards.length / columns);
  const spacer = (position: string) => parseFloat(container.querySelector<HTMLElement>(`[data-window-spacer="${position}"]`)!.style.height);
  // The measured card is 100px, each row stride includes the 14px grid gap.
  expect(spacer('top')).toBe(0);
  expect(spacer('bottom')).toBe((totalRows - visibleRows) * 114);
  expect(spacer('top') + visibleRows * 114 + spacer('bottom')).toBe(totalRows * 114);
});

it.each([false, true])('hides unmeasured grids until synchronous layout provides columns (grouped=%s)', grouped => {
  width = 0;
  const { container } = render(<Harness grouped={grouped} count={2} />);
  const grid = container.querySelector<HTMLElement>('.grid.gap-3\\.5')!;
  expect(grid.parentElement).toHaveStyle({ visibility: 'hidden' });
  act(() => { width = 800; resizeCallbacks.forEach(callback => callback()); });
  expect(grid.parentElement).toHaveStyle({ visibility: 'visible' });
  expect(grid.style.gridTemplateColumns).toBe('repeat(4, minmax(0, 1fr))');
});
it('measures the first layout without waiting for ResizeObserver', () => {
  // Layout width is valid even when clientWidth has not supplied an initial value.
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(0);
  const { container } = render(<Harness />);
  const grid = container.querySelector<HTMLElement>('.grid.gap-3\\.5')!;
  expect(grid.style.gridTemplateColumns).toBe('repeat(4, minmax(0, 1fr))');
  expect(grid.parentElement).toHaveStyle({ visibility: 'visible' });
});

it.each([false, true])('disables native image dragging on model cards (grouped=%s)', grouped => {
  models[0].thumbnailImage = 'data:image/png;base64,eA==';
  try {
    const { container } = render(<Harness grouped={grouped} count={2} />);
    const images = container.querySelectorAll('[data-model-id] img');
    expect(images.length).toBeGreaterThan(0);
    images.forEach(image => expect(image).toHaveAttribute('draggable', 'false'));
  } finally { models[0].thumbnailImage = null; }
});
