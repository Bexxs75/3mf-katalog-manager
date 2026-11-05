import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { createElement, type ReactNode } from 'react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { useBundleActions } from './useBundleActions';
import type { useBundles } from './useBundles';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function setup(createBundle: (name: string, kind: string, ids: string[]) => Promise<unknown>) {
  const bundleStore = { bundles: [], createBundle } as unknown as ReturnType<typeof useBundles>;
  const wrapper = ({ children }: { children: ReactNode }) => createElement(LanguageProvider, null, children);
  return renderHook(() => useBundleActions({ bundleStore, models: [], selection: new Set(), selectIds: vi.fn(), collapse: true }), { wrapper });
}
const items = [
  { name: 'Lampe', kind: 'variants' as const, fileIds: ['1', '2'] },
  { name: 'Schiff', kind: 'parts' as const, fileIds: ['3', '4', '5'] },
];

describe('useBundleActions.createSuggested', () => {
  it('creates the bundles in order without selecting or editing anything, and reports the total', async () => {
    const createBundle = vi.fn().mockResolvedValue({ id: 'b' });
    const { result } = setup(createBundle);
    let made = 0;
    await act(async () => { made = await result.current.createSuggested(items); });
    expect(made).toBe(2);
    expect(createBundle.mock.calls).toEqual([['Lampe', 'variants', ['1', '2']], ['Schiff', 'parts', ['3', '4', '5']]]);
    expect(result.current.toast?.text).toBe('2 Bündel angelegt (5 Modelle).');
  });

  it('stops at the first failure, rethrows it and still reports what was created before', async () => {
    const createBundle = vi.fn().mockResolvedValueOnce({ id: 'b' }).mockRejectedValueOnce(new Error('full'));
    const { result } = setup(createBundle);
    await act(async () => { await expect(result.current.createSuggested(items)).rejects.toThrow('full'); });
    expect(createBundle).toHaveBeenCalledTimes(2);
    expect(result.current.toast?.text).toBe('1 Bündel angelegt (2 Modelle).');
  });

  it('creates nothing and says nothing when the session may not edit', async () => {
    const { result } = setup(vi.fn().mockResolvedValue(undefined));
    let made = -1;
    await act(async () => { made = await result.current.createSuggested(items); });
    expect(made).toBe(0);
    expect(result.current.toast).toBeNull();
  });
});
