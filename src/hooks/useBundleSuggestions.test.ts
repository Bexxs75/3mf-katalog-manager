import { beforeEach, describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { useBundleSuggestions } from './useBundleSuggestions';
import { makeModelFile } from '../test/factories';
import type { Bundle, ModelFile } from '../types';

beforeEach(() => localStorage.clear());

const old = [makeModelFile({ id: 'o1', name: 'Vase_v1.3mf' }), makeModelFile({ id: 'o2', name: 'Vase_v2.3mf' })];
const fresh = [makeModelFile({ id: 'n1', name: 'Regal_v1.3mf' }), makeModelFile({ id: 'n2', name: 'Regal_v2.3mf' })];

function setup(models: ModelFile[], enabled = true, bundles: Bundle[] = []) {
  return renderHook(({ models: m }) => useBundleSuggestions({ models: m, bundles, enabled }), { initialProps: { models } });
}

describe('useBundleSuggestions', () => {
  it('offers every open suggestion to the menu but notices only those with newly added files', () => {
    const { result } = setup([...old, ...fresh]);
    expect(result.current.open).toHaveLength(2);
    expect(result.current.noticeCount).toBe(0);
    act(() => result.current.notifyAdded(['n1']));
    expect(result.current.noticeCount).toBe(1);
  });

  it('counts the notice once the added files appear in the list, not before', () => {
    const { result, rerender } = setup(old);
    act(() => result.current.notifyAdded(['n1', 'n2']));
    expect(result.current.noticeCount).toBe(0);
    rerender({ models: [...old, ...fresh] });
    expect(result.current.noticeCount).toBe(1);
  });

  it('clears the notice on "Later" and on opening the dialog, which keeps the menu entry', () => {
    const { result } = setup([...old, ...fresh]);
    act(() => result.current.notifyAdded(['n1']));
    act(() => result.current.dismissNotice());
    expect(result.current.noticeCount).toBe(0);
    act(() => result.current.notifyAdded(['n1']));
    act(() => result.current.openDialog(null));
    expect(result.current.noticeCount).toBe(0);
    expect(result.current.dialogOpen).toBe(true);
    expect(result.current.open).toHaveLength(2);
  });

  it('shows no notice when the hint is switched off, and remembers that', () => {
    const { result } = setup([...old, ...fresh]);
    act(() => result.current.setHint(false));
    act(() => result.current.notifyAdded(['n1']));
    expect(result.current.noticeCount).toBe(0);
    expect(setup([...old, ...fresh]).result.current.hint).toBe(false);
  });

  it('moves a dismissed group out of the open list and keeps it across a reload', () => {
    const { result } = setup([...old, ...fresh]);
    const key = result.current.open[0].key;
    act(() => result.current.dismiss(key));
    expect(result.current.open.map(suggestion => suggestion.key)).not.toContain(key);
    expect(result.current.dismissed.map(suggestion => suggestion.key)).toEqual([key]);
    expect(setup([...old, ...fresh]).result.current.dismissed).toHaveLength(1);
    act(() => result.current.restore(key));
    expect(result.current.open).toHaveLength(2);
  });

  it('suggests nothing without the right to change bundles', () => {
    const { result } = setup([...old, ...fresh], false);
    expect(result.current.open).toEqual([]);
    act(() => result.current.notifyAdded(['n1']));
    expect(result.current.noticeCount).toBe(0);
  });
});
