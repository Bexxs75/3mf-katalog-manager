import { describe, expect, it } from 'vitest';
import { lockedKind, parsePrintTechnology, parsePrintTechnologyState, showsKind } from './printTechnology';

describe('printTechnology', () => {
  it('treats anything unknown as both so nothing is hidden by accident', () => {
    for (const v of ['filament', 'resin', 'both']) expect(parsePrintTechnology(v)).toBe(v);
    for (const v of [undefined, null, '', 'Resin', 'laser', 3, {}]) expect(parsePrintTechnology(v)).toBe('both');
  });

  it('reads a damaged answer as "asked, both" so it neither hides nor nags', () => {
    expect(parsePrintTechnologyState({ value: 'resin', asked: true })).toEqual({ value: 'resin', asked: true });
    expect(parsePrintTechnologyState({ value: 'both', asked: false })).toEqual({ value: 'both', asked: false });
    for (const raw of [undefined, null, [], 'resin', 5]) expect(parsePrintTechnologyState(raw)).toEqual({ value: 'both', asked: true });
  });

  it('shows a kind only when the choice includes it', () => {
    expect([showsKind('both', 'filament'), showsKind('both', 'resin')]).toEqual([true, true]);
    expect([showsKind('filament', 'filament'), showsKind('filament', 'resin')]).toEqual([true, false]);
    expect([showsKind('resin', 'filament'), showsKind('resin', 'resin')]).toEqual([false, true]);
  });

  it('locks the material view to the single chosen kind', () => {
    expect(lockedKind('both')).toBeNull();
    expect(lockedKind('filament')).toBe('filament');
    expect(lockedKind('resin')).toBe('resin');
  });
});
