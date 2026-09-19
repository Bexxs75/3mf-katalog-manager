import { describe, expect, it, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useCollapsedFolders } from './useCollapsedFolders';

beforeEach(() => localStorage.clear());

describe('useCollapsedFolders', () => {
  it('starts with nothing collapsed', () => {
    const { result } = renderHook(() => useCollapsedFolders());
    expect(result.current.isCollapsed('a')).toBe(false);
  });

  it('toggle collapses and un-collapses a folder', () => {
    const { result } = renderHook(() => useCollapsedFolders());
    act(() => result.current.toggle('a'));
    expect(result.current.isCollapsed('a')).toBe(true);
    act(() => result.current.toggle('a'));
    expect(result.current.isCollapsed('a')).toBe(false);
  });

  it('persists collapsed state across hook instances (localStorage)', () => {
    const first = renderHook(() => useCollapsedFolders());
    act(() => first.result.current.toggle('a'));
    const second = renderHook(() => useCollapsedFolders());
    expect(second.result.current.isCollapsed('a')).toBe(true);
  });

  it('tolerates corrupted localStorage content', () => {
    localStorage.setItem('3mf-katalog-collapsed-folders', 'not-json');
    const { result } = renderHook(() => useCollapsedFolders());
    expect(result.current.isCollapsed('a')).toBe(false);
  });
});
