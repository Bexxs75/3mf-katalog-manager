import { fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ModelGrid } from './ModelGrid';
import { ModelList } from './ModelList';
import { LanguageProvider } from '../i18n/LanguageContext';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { ModelLayoutProvider } from '../hooks/ModelLayoutContext';
import { BundleCardsContext, type BundleCardsSettings } from '../hooks/BundleCardsContext';
import { makeModelFile } from '../test/factories';
import type { Bundle, BundleKind } from '../types';

vi.mock('../lib/api/files', () => ({ listFileImages: vi.fn().mockResolvedValue([]) }));
beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('3mf-katalog-language', 'de');
  // The grid stays hidden until it has measured a width, which jsdom never reports.
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ top: 0, left: 0, width: 800, height: 100, right: 800, bottom: 100 } as DOMRect);
});
afterEach(() => vi.restoreAllMocks());

const noop = vi.fn();
const handlers = () => ({
  onSelect: vi.fn(), onOpenDetail: vi.fn(), onContextMenu: vi.fn(), onToggleFavorite: noop, onToggleBulkSelect: vi.fn(),
  displayPreference: 'thumbnail' as const,
});
const alone = makeModelFile({ id: 'solo', name: 'Solo.stl' });
const first = makeModelFile({ id: 'v1', name: 'Holder big.3mf', estimatedWeightG: 12, weightSource: 'slicer', materials: [{ name: 'PLA', displayColor: null }], tags: ['wall'] });
const second = makeModelFile({ id: 'v2', name: 'Holder small.3mf', printStatus: 'printed', fileSizeBytes: 2048 });
const third = makeModelFile({ id: 'v3', name: 'Holder tiny.stl' });
const models = [alone, first, second, third];

function bundle(kind: BundleKind = 'variants'): Bundle {
  return {
    id: 'b1', name: 'Holder', kind, createdAt: '2026-11-01T00:00:00Z',
    members: [
      { fileId: 'v1', position: 0, label: 'big', inTrash: false },
      { fileId: 'v2', position: 1, label: null, inTrash: false },
      { fileId: 'v3', position: 2, label: null, inTrash: false },
    ],
  };
}
const settings = (extra: Partial<BundleCardsSettings> = {}): BundleCardsSettings => ({ bundles: [bundle()], collapse: true, ...extra });
const wrap = (node: React.ReactNode, value: BundleCardsSettings | null) => (
  <LanguageProvider><UiDensityProvider><ModelLayoutProvider>
    <BundleCardsContext.Provider value={value}>{node}</BundleCardsContext.Provider>
  </ModelLayoutProvider></UiDensityProvider></LanguageProvider>
);
const rowsOf = () => [...document.querySelectorAll<HTMLElement>('[role="row"][data-model-id]')];

describe.each(['compact', 'comfort'])('bundle card in the grid (%s)', density => {
  beforeEach(() => localStorage.setItem('3mf-katalog-density', density));
  const grid = (h = handlers(), value: BundleCardsSettings | null = settings(), extra: Record<string, unknown> = {}) =>
    wrap(<ModelGrid {...h} models={models} selectedId={null} selectedForBulk={new Set()} {...extra} />, value);

  it('shows the three variants as one card next to the single file, with a name that says what it is', () => {
    render(grid());
    expect(rowsOf().map(row => row.getAttribute('data-model-id'))).toEqual(['solo', 'v1']);
    const card = screen.getByRole('row', { name: 'Bündel Holder, 3 Varianten' });
    expect(card).toHaveAttribute('data-bundle-id', 'b1');
    expect(within(card).getByText('3 Varianten')).toBeInTheDocument();
    expect(screen.getByRole('grid')).toHaveAttribute('aria-rowcount', '2');
  });

  it('keeps every file on its own card, marked as part of the bundle, when the switch is off', () => {
    render(grid(handlers(), settings({ collapse: false })));
    expect(rowsOf()).toHaveLength(4);
    expect(document.querySelector('[data-bundle-id]')).toBeNull();
    const marked = screen.getByRole('row', { name: /Holder big.*Teil von „Holder“/ });
    expect(within(marked).getByText('Teil von „Holder“')).toBeInTheDocument();
    expect(screen.getByRole('row', { name: 'Solo.stl' })).toBeInTheDocument();
  });

  it('says "2 von 3" when a filter hides a member and still counts the hidden one', () => {
    render(wrap(<ModelGrid {...handlers()} models={[alone, first, second]} selectedId={null} selectedForBulk={new Set()} />, settings()));
    expect(screen.getByRole('row', { name: 'Bündel Holder, 2 von 3 Varianten' })).toBeInTheDocument();
  });

  it('shows how far the parts are printed', () => {
    render(grid(handlers(), settings({ bundles: [bundle('parts')] })));
    expect(screen.getByRole('row', { name: 'Bündel Holder, 3 Teile · 1 von 3 gedruckt' })).toBeInTheDocument();
  });

  it('gives a bundle card the same height as a single card, also while a print data filter makes room', () => {
    render(grid());
    const heights = [...document.querySelectorAll<HTMLElement>('[data-card-metadata]')].map(el => el.style.height);
    expect(heights).toHaveLength(2);
    expect(heights[0]).not.toBe('');
    expect(new Set(heights).size).toBe(1);
  });

  it('opens the title model on click and on double click', () => {
    const h = handlers();
    render(grid(h));
    const card = screen.getByRole('row', { name: /^Bündel Holder/ });
    fireEvent.click(card);
    expect(h.onSelect).toHaveBeenCalledWith('v1');
    fireEvent.doubleClick(card);
    expect(h.onOpenDetail).toHaveBeenCalledWith('v1');
  });

  it('opens the context menu for the bundle, not for the title model alone, and for a single card without a bundle', () => {
    const h = handlers();
    render(grid(h));
    fireEvent.contextMenu(screen.getByRole('row', { name: /^Bündel Holder/ }), { clientX: 5, clientY: 6 });
    expect(h.onContextMenu).toHaveBeenLastCalledWith('v1', 5, 6, 'b1');
    fireEvent.contextMenu(screen.getByRole('row', { name: 'Solo.stl' }), { clientX: 7, clientY: 8 });
    expect(h.onContextMenu).toHaveBeenLastCalledWith('solo', 7, 8);
  });

  it('marks the card as open when any shown member is open', () => {
    render(grid(handlers(), settings(), { selectedId: 'v3' }));
    expect(screen.getByRole('row', { name: /^Bündel Holder/ })).toHaveAttribute('aria-current', 'true');
    expect(screen.getByRole('row', { name: 'Solo.stl' })).not.toHaveAttribute('aria-current');
  });

  it('selects all living members through its checkbox, also those a filter hides', () => {
    const h = handlers();
    render(wrap(<ModelGrid {...h} models={[alone, first, second]} selectedId={null} selectedForBulk={new Set(['v1'])} />, settings()));
    const card = screen.getByRole('row', { name: /^Bündel Holder/ });
    fireEvent.click(within(card).getByRole('checkbox'));
    expect(h.onToggleBulkSelect.mock.calls.map(call => call[0])).toEqual(['v2', 'v3']);
  });

  it('clears all members when they are all selected', () => {
    const h = handlers();
    render(grid(h, settings(), { selectedForBulk: new Set(['v1', 'v2', 'v3']) }));
    const card = screen.getByRole('row', { name: /^Bündel Holder/ });
    expect(within(card).getByRole('checkbox')).toBeChecked();
    expect(card).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(within(card).getByRole('checkbox'));
    expect(h.onToggleBulkSelect.mock.calls.map(call => call[0])).toEqual(['v1', 'v2', 'v3']);
  });

  it('keeps the old grid without any provider', () => {
    render(grid(handlers(), null));
    expect(rowsOf()).toHaveLength(4);
    expect(document.querySelector('[data-part-of]')).toBeNull();
  });
});

describe('bundle row in the list', () => {
  it('opens the context menu for the bundle on its row', () => {
    const h = handlers();
    render(wrap(<ModelList {...h} models={models} selectedId={null} selectedForBulk={new Set()} />, settings()));
    fireEvent.contextMenu(screen.getByRole('row', { name: /^Bündel Holder/ }), { clientX: 3, clientY: 4 });
    expect(h.onContextMenu).toHaveBeenLastCalledWith('v1', 3, 4, 'b1');
  });

  const list = (h = handlers(), value: BundleCardsSettings | null = settings()) =>
    wrap(<ModelList {...h} models={models} selectedId={null} selectedForBulk={new Set()} />, value);

  it('is one row with a button that unfolds the members', () => {
    render(list());
    const row = screen.getByRole('row', { name: 'Bündel Holder, 3 Varianten' });
    const toggle = within(row).getByRole('button', { name: 'Bündel aufklappen: Holder' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(document.querySelectorAll('[data-bundle-member]')).toHaveLength(0);
    expect(screen.getByRole('grid')).toHaveAttribute('aria-rowcount', '3');

    toggle.focus();
    fireEvent.click(toggle);
    // The same button stays in the page, so the focus does not jump away.
    expect(toggle).toBeInTheDocument();
    expect(document.activeElement).toBe(toggle);
    expect(within(row).getByRole('button', { name: 'Bündel zuklappen: Holder' })).toHaveAttribute('aria-expanded', 'true');
    const members = [...document.querySelectorAll<HTMLElement>('[data-bundle-member]')];
    expect(members.map(member => member.getAttribute('data-member-id'))).toEqual(['v1', 'v2', 'v3']);
    expect(members[0]).toHaveAccessibleName('Holder big.3mf, big, Teil von Bündel Holder');
    expect(screen.getByRole('grid')).toHaveAttribute('aria-rowcount', '6');
    expect(members.map(member => member.getAttribute('aria-rowindex'))).toEqual(['4', '5', '6']);

    fireEvent.click(within(row).getByRole('button', { name: 'Bündel zuklappen: Holder' }));
    expect(document.querySelectorAll('[data-bundle-member]')).toHaveLength(0);
  });

  it('does not select the title model when the arrow button is clicked', () => {
    const h = handlers();
    render(list(h));
    fireEvent.click(screen.getByRole('button', { name: 'Bündel aufklappen: Holder' }));
    expect(h.onSelect).not.toHaveBeenCalled();
  });

  it('selects a member row on click, and with the space key from the keyboard', () => {
    const h = handlers();
    render(list(h));
    fireEvent.click(screen.getByRole('button', { name: 'Bündel aufklappen: Holder' }));
    const member = document.querySelector<HTMLElement>('[data-member-id="v2"]')!;
    expect(member).toHaveAttribute('tabindex', '0');
    fireEvent.click(member);
    expect(h.onSelect).toHaveBeenLastCalledWith('v2');
    fireEvent.keyDown(member, { key: ' ' });
    expect(h.onSelect).toHaveBeenCalledTimes(2);
  });

  it('keeps the arrow-key order on the cards, one entry per row', () => {
    render(list());
    expect([...document.querySelectorAll('[data-model-id]')].map(row => row.getAttribute('data-model-id'))).toEqual(['solo', 'v1']);
  });

  it('marks a file of a bundle that is not shown as a card', () => {
    render(list(handlers(), settings({ collapse: false })));
    expect(document.querySelectorAll('[data-part-of]')).toHaveLength(3);
    expect(screen.getByRole('row', { name: /Holder small.*Teil von „Holder“/ })).toBeInTheDocument();
  });

  it('says "war Teil von Bündel" for files in the trash', () => {
    render(list(handlers(), settings({ collapse: false, trash: true })));
    expect(screen.getAllByText('war Teil von Bündel „Holder“')).toHaveLength(3);
  });
});
