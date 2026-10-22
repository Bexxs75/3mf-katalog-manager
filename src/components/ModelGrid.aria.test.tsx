import { act, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useCallback, useRef, useState } from 'react';
import { fireEvent } from '@testing-library/react';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { SelectionAnnouncer } from './SelectionAnnouncer';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ModelLayoutProvider } from '../hooks/ModelLayoutContext';
import { shouldIgnoreCatalogShortcut } from '../lib/keyboardGuard';
import { de } from '../i18n/de';
import { makeModelFile } from '../test/factories';

vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn().mockResolvedValue([]) }));
beforeEach(() => localStorage.clear());
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

const noop = vi.fn();
const base = { onSelect: noop, onOpenDetail: noop, onContextMenu: noop, onToggleFavorite: noop, onToggleBulkSelect: noop, displayPreference: 'thumbnail' as const };
const three = ['a', 'b', 'c'].map(id => makeModelFile({ id, name: `Model ${id}.stl` }));
const wrap = (node: React.ReactNode) => <LanguageProvider><UiDensityProvider><ModelLayoutProvider>{node}</ModelLayoutProvider></UiDensityProvider></LanguageProvider>;

describe.each(['compact', 'comfort', 'list'])('grid semantics (%s)', view => {
  const ui = (extra = {}) => wrap(view === 'list'
    ? <ModelList {...base} models={three} selectedId="a" selectedForBulk={new Set(['b'])} {...extra} />
    : <ModelGrid {...base} models={three} selectedId="a" selectedForBulk={new Set(['b'])} {...extra} />);
  beforeEach(() => {
    localStorage.setItem('3mf-katalog-density', view);
    // The grid stays visibility:hidden until it has measured a width, which jsdom never reports.
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ top: 0, left: 0, width: 800, height: 100, right: 800, bottom: 100 } as DOMRect);
  });

  it('exposes a named multiselectable grid with one row per model and its selection state', () => {
    render(ui());
    const grid = screen.getByRole('grid', { name: de.modelListLabel });
    expect(grid).toHaveAttribute('aria-multiselectable', 'true');
    const rows = within(grid).getAllByRole('row').filter(row => row.hasAttribute('data-model-id'));
    expect(rows).toHaveLength(3);
    expect(rows.map(row => row.getAttribute('aria-selected'))).toEqual(['true', 'true', 'false']);
    expect(rows[0]).toHaveAccessibleName('Model a.stl');
    expect(within(rows[0]).getAllByRole('gridcell').length).toBeGreaterThan(0);
  });

  it('uses a custom label for grouped sections', () => {
    render(ui({ label: 'Benchy' }));
    expect(screen.getByRole('grid', { name: 'Benchy' })).toBeInTheDocument();
  });

  it('does not make a focused row swallow the catalog shortcuts', () => {
    render(ui());
    const row = document.querySelector<HTMLElement>('[data-model-id="b"]')!;
    row.focus();
    expect(document.activeElement).toBe(row);
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { key: 'ArrowRight' }))).toBe(false);
    // Other grids stay protected: they own their arrow keys.
    const foreign = document.createElement('div'); foreign.setAttribute('role', 'grid'); foreign.tabIndex = 0;
    document.body.append(foreign); foreign.focus();
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { key: 'ArrowRight' }))).toBe(true);
    foreign.remove();
  });
});

it('list rows count the header row and name the checkbox column', () => {
  localStorage.setItem('3mf-katalog-density', 'list');
  render(wrap(<ModelList {...base} models={three} selectedId={null} selectedForBulk={new Set()} />));
  expect(screen.getByRole('grid')).toHaveAttribute('aria-rowcount', '4');
  expect(screen.getAllByRole('columnheader').map(h => h.textContent)).toEqual([de.columnSelect, de.columnName, de.columnTags, de.columnVolume, de.columnSize]);
  expect(document.querySelectorAll('[data-model-id]')[0]).toHaveAttribute('aria-rowindex', '2');
});

describe('selection announcements', () => {
  it('stays silent on first render and announces count changes and clearing', () => {
    const { container, rerender } = render(wrap(<SelectionAnnouncer count={0} />));
    const region = container.querySelector('[aria-live="polite"]')!;
    expect(region).toHaveTextContent('');
    rerender(wrap(<SelectionAnnouncer count={1} />));
    expect(region).toHaveTextContent('1 Modell ausgewählt');
    rerender(wrap(<SelectionAnnouncer count={3} />));
    expect(region).toHaveTextContent('3 Modelle ausgewählt');
    rerender(wrap(<SelectionAnnouncer count={0} />));
    expect(region).toHaveTextContent(de.selectionCleared);
  });
});

describe('virtualized windows', () => {
  beforeEach(() => {
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(400);
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      const top = this.dataset.catalogScroller !== undefined ? 0 : -(this.closest<HTMLElement>('[data-catalog-scroller]')?.scrollTop ?? 0);
      return { top, left: 0, width: 800, height: this.dataset.modelId ? 100 : 0, right: 800, bottom: 100 } as DOMRect;
    });
    vi.stubGlobal('ResizeObserver', class { observe() {} disconnect() {} });
  });
  const many = Array.from({ length: 200 }, (_, i) => makeModelFile({ id: String(i), name: `Model ${i}` }));
  function Harness({ list }: { list: boolean }) {
    const containerRef = useRef<HTMLDivElement>(null);
    const props = { ...base, models: many, selectedId: '0', selectedForBulk: new Set<string>(), containerRef };
    return wrap(<div ref={containerRef} data-catalog-scroller>{list ? <ModelList {...props} /> : <ModelGrid {...props} />}</div>);
  }
  it.each([false, true])('reports the full size while rendering a slice and keeps absolute indexes (list=%s)', async list => {
    localStorage.setItem('3mf-katalog-density', list ? 'list' : 'compact');
    const { container } = render(<Harness list={list} />);
    await waitFor(() => expect(container.querySelectorAll('[data-model-id]').length).toBeGreaterThan(0));
    const rendered = container.querySelectorAll<HTMLElement>('[data-model-id]');
    expect(rendered.length).toBeLessThan(200);
    expect(screen.getByRole('grid')).toHaveAttribute('aria-rowcount', String(list ? 201 : 200));
    const offset = list ? 2 : 1;
    rendered.forEach((row, i) => expect(row).toHaveAttribute('aria-rowindex', String(Number(rendered[0].dataset.modelId) + i + offset)));
    const scroller = container.querySelector<HTMLElement>('[data-catalog-scroller]')!;
    await act(async () => { scroller.scrollTop = 3000; scroller.dispatchEvent(new Event('scroll')); await new Promise(r => requestAnimationFrame(r)); });
    const later = container.querySelector<HTMLElement>('[data-model-id]')!;
    expect(Number(later.dataset.modelId)).toBeGreaterThan(0);
    expect(later).toHaveAttribute('aria-rowindex', String(Number(later.dataset.modelId) + offset));
  });
});

it('keeps arrow, Space and focus working while a grid row has DOM focus', async () => {
  localStorage.setItem('3mf-katalog-density', 'compact');
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ top: 0, left: 0, width: 800, height: 100, right: 800, bottom: 100 } as DOMRect);
  function Keyboard() {
    const [selectedId, setSelectedId] = useState<string | null>('a');
    const [bulk, setBulk] = useState(new Set<string>());
    const toggle = useCallback((id: string) => setBulk(prev => { const next = new Set(prev); if (!next.delete(id)) next.add(id); return next; }), []);
    useKeyboardShortcuts({ filteredIds: three.map(m => m.id), selectedId, selectModel: setSelectedId, hasBulkSelection: bulk.size > 0,
      openBulkDeleteConfirm: noop, navigationEnabled: true, toggleBulkSelect: toggle });
    return <><ModelGrid {...base} models={three} selectedId={selectedId} selectedForBulk={bulk} windowed={false} /><SelectionAnnouncer count={bulk.size} /></>;
  }
  const { container } = render(wrap(<Keyboard />));
  const row = (id: string) => container.querySelector<HTMLElement>(`[data-model-id="${id}"]`)!;
  row('a').focus();
  fireEvent.keyDown(row('a'), { key: 'ArrowRight' });
  await waitFor(() => expect(row('b')).toHaveAttribute('aria-selected', 'true'));
  await waitFor(() => expect(document.activeElement).toBe(row('b')));
  fireEvent.keyDown(row('b'), { key: ' ' });
  await waitFor(() => expect(container.querySelector('[data-selection-announcer]')).toHaveTextContent('1 Modell ausgewählt'));
  expect(document.activeElement).toBe(row('b'));
});
