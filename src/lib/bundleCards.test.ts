import { describe, expect, it } from 'vitest';
import {
  bundleCardMeta, bundleCardSummary, cardMemberIds, cardNavId, cardSelectionState, cardToggleIds, isCardOpen,
} from './bundleCards';
import { buildCards, type BundleCard } from './bundles';
import { makeModelFile } from '../test/factories';
import type { Bundle, BundleKind, BundleMember, ModelFile } from '../types';

const member = (fileId: string, position: number, extra: Partial<BundleMember> = {}): BundleMember =>
  ({ fileId, position, label: null, inTrash: false, ...extra });
const bundle = (kind: BundleKind, members: BundleMember[]): Bundle =>
  ({ id: 'b', name: 'Gearbox', kind, createdAt: '2026-11-01T00:00:00Z', members });

const a = makeModelFile({ id: 'a', materials: [{ name: 'PLA', displayColor: null }], estimatedWeightG: 10, weightSource: 'slicer', printStatus: 'printed' });
const b = makeModelFile({ id: 'b2', materials: [{ name: 'PETG', displayColor: null }], estimatedWeightG: 30, weightSource: 'estimated' });
const c = makeModelFile({ id: 'c', materials: [{ name: 'PLA', displayColor: null }], estimatedWeightG: null });

function card(kind: BundleKind, models: ModelFile[], members = [member('a', 0), member('b2', 1), member('c', 2)]): BundleCard {
  const cards = buildCards(models, [bundle(kind, members)], { collapse: true });
  const found = cards.find((item): item is BundleCard => item.type === 'bundle');
  if (!found) throw new Error('no bundle card');
  return found;
}

describe('cardNavId', () => {
  it('names a bundle card by its title model and a single card by its file', () => {
    expect(cardNavId(card('parts', [a, b, c]))).toBe('a');
    expect(cardNavId({ type: 'model', key: 'c', model: c, partOf: null })).toBe('c');
  });
});

describe('bundleCardSummary', () => {
  it('counts all members and the printed ones', () => {
    expect(bundleCardSummary(card('parts', [a, b, c]))).toEqual({ kind: 'parts', shown: 3, total: 3, printed: 1 });
  });

  it('counts only the shown members as shown and printed, but all living ones as total', () => {
    expect(bundleCardSummary(card('variants', [b, c]))).toEqual({ kind: 'variants', shown: 2, total: 3, printed: 0 });
  });

  it('does not count a member in the trash', () => {
    const trashed = card('parts', [a, b], [member('a', 0), member('b2', 1), member('c', 2, { inTrash: true })]);
    expect(bundleCardSummary(trashed)).toMatchObject({ shown: 2, total: 2 });
  });
});

describe('bundleCardMeta', () => {
  it('shows the title model for variants', () => {
    expect(bundleCardMeta(card('variants', [a, b, c]))).toEqual({ materials: ['PLA'], weightG: 10, weightFromSlicer: true });
  });

  it('sums the parts, lists each material once and marks a mixed weight as an estimate', () => {
    expect(bundleCardMeta(card('parts', [a, b, c]))).toEqual({ materials: ['PLA', 'PETG'], weightG: 40, weightFromSlicer: false });
  });

  it('has no weight when no part knows one', () => {
    const unknown = [makeModelFile({ id: 'a' }), makeModelFile({ id: 'b2' })];
    expect(bundleCardMeta(card('parts', unknown, [member('a', 0), member('b2', 1)]))).toMatchObject({ weightG: null, weightFromSlicer: false });
  });
});

describe('card selection', () => {
  const parts = card('parts', [a, b]);

  it('lists the living members, also those a filter hides', () => {
    expect(cardMemberIds(parts)).toEqual(['a', 'b2', 'c']);
  });

  it('is none, some or all', () => {
    expect(cardSelectionState(parts, new Set())).toBe('none');
    expect(cardSelectionState(parts, new Set(['a']))).toBe('some');
    expect(cardSelectionState(parts, new Set(['a', 'b2', 'c', 'x']))).toBe('all');
  });

  it('toggles so that a partly selected card ends up fully selected', () => {
    expect(cardToggleIds(parts, new Set(['a']))).toEqual(['b2', 'c']);
  });

  it('toggles a fully selected card empty and an empty one full', () => {
    expect(cardToggleIds(parts, new Set(['a', 'b2', 'c']))).toEqual(['a', 'b2', 'c']);
    expect(cardToggleIds(parts, new Set())).toEqual(['a', 'b2', 'c']);
  });
});

describe('isCardOpen', () => {
  it('is true for a bundle card when any shown member is open', () => {
    const shown = card('parts', [a, b]);
    expect(isCardOpen(shown, 'b2')).toBe(true);
    expect(isCardOpen(shown, 'c')).toBe(false);
    expect(isCardOpen(shown, null)).toBe(false);
  });

  it('is true for a single card of the open file', () => {
    const single = { type: 'model' as const, key: 'c', model: c, partOf: null };
    expect(isCardOpen(single, 'c')).toBe(true);
    expect(isCardOpen(single, 'a')).toBe(false);
  });
});
