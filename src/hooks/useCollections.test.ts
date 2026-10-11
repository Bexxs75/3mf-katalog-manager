import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useCollections } from './useCollections';

const language = vi.hoisted(() => ({ current: 'de' }));
vi.mock('../i18n/LanguageContext', () => ({ useLanguage: () => ({ language: language.current }) }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); language.current = 'de'; });

describe('useCollections', () => {
  it('loads collections on mount', async () => {
    vi.mocked(invoke).mockResolvedValue([{ id: 'c1', name: 'A', modelCount: 2 }]);
    const { result } = renderHook(() => useCollections());
    await waitFor(() => expect(result.current.collections).toHaveLength(1));
    expect(invoke).toHaveBeenCalledWith('list_collections');
  });

  it('sorts names naturally with accents and case ignored without changing the API result', async () => {
    const names = ['Zebra', 'Kiste 10', 'Öfen', 'banana', 'Kiste 2', 'äpfel', 'Apfel'];
    const records = names.map((name, index) => ({ id: String(index), name, modelCount: 0 }));
    vi.mocked(invoke).mockResolvedValue(records);
    const { result } = renderHook(() => useCollections());
    await waitFor(() => expect(result.current.collections.map(collection => collection.name)).toEqual([
      'äpfel', 'Apfel', 'banana', 'Kiste 2', 'Kiste 10', 'Öfen', 'Zebra',
    ]));
    expect(records.map(collection => collection.name)).toEqual(names);
  });

  it('reorders the existing names when the language changes without refetching', async () => {
    vi.mocked(invoke).mockResolvedValue([
      { id: '1', name: 'nube', modelCount: 0 },
      { id: '2', name: 'ñandú', modelCount: 0 },
    ]);
    const { result, rerender } = renderHook(() => useCollections());
    await waitFor(() => expect(result.current.collections.map(collection => collection.name)).toEqual(['ñandú', 'nube']));
    language.current = 'es';
    rerender();
    expect(result.current.collections.map(collection => collection.name)).toEqual(['nube', 'ñandú']);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it('inserts a newly created collection at its alphabetical position', async () => {
    const records = [{ id: '1', name: 'Zebra', modelCount: 0 }];
    vi.mocked(invoke).mockImplementation(command => {
      if (command === 'create_collection') {
        records.push({ id: '2', name: 'Äpfel', modelCount: 0 });
        return Promise.resolve(undefined);
      }
      return Promise.resolve([...records]);
    });
    const { result } = renderHook(() => useCollections());
    await waitFor(() => expect(result.current.collections).toHaveLength(1));
    await act(async () => result.current.createCollection('Äpfel'));
    expect(result.current.collections.map(collection => collection.name)).toEqual(['Äpfel', 'Zebra']);
  });

  it('setActiveCollection loads that collection\'s files', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_collections') return Promise.resolve([]);
      if (cmd === 'list_collection_files') return Promise.resolve([{ id: 'm1' }]);
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useCollections());
    act(() => result.current.setActiveCollection('c1'));
    await waitFor(() => expect(result.current.collectionModels).toEqual([{ id: 'm1' }]));
    expect(invoke).toHaveBeenCalledWith('list_collection_files', { collectionId: 'c1' });
  });

  it('setActiveCollection(null) clears collectionModels', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const { result } = renderHook(() => useCollections());
    act(() => result.current.setActiveCollection(null));
    expect(result.current.collectionModels).toEqual([]);
  });

  it('createCollection calls create_collection and refreshes the list', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const { result } = renderHook(() => useCollections());
    await act(async () => result.current.createCollection('New'));
    expect(invoke).toHaveBeenCalledWith('create_collection', { name: 'New' });
  });

  it('bulkAddToCollection adds files and refreshes the active collection', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_collections') return Promise.resolve([]);
      if (cmd === 'list_collection_files') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const { result } = renderHook(() => useCollections());
    act(() => result.current.setActiveCollection('c1'));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_collection_files', { collectionId: 'c1' }));
    vi.mocked(invoke).mockClear();
    await act(async () => result.current.bulkAddToCollection(['m1', 'm2'], 'c1'));
    expect(invoke).toHaveBeenCalledWith('add_files_to_collection', { collectionId: 'c1', fileIds: ['m1', 'm2'] });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('list_collection_files', { collectionId: 'c1' }));
  });
});

it('ignores late members of another collection and older refreshes', async () => {
  const pending: Array<{ id: string; resolve: (files: unknown[]) => void }> = [];
  vi.mocked(invoke).mockImplementation((command, args) => command === 'list_collection_files'
    ? new Promise(resolve => pending.push({ id: (args as {collectionId: string}).collectionId, resolve }))
    : Promise.resolve([]));
  const { result } = renderHook(() => useCollections());
  act(() => result.current.setActiveCollection('a'));
  act(() => result.current.setActiveCollection('b'));
  await act(async () => pending[1].resolve([{id: 'b1'}]));
  await act(async () => pending[0].resolve([]));
  expect(result.current.collectionModels).toEqual([{id: 'b1'}]);
  act(() => { void result.current.refreshCollectionModels('b'); });
  act(() => { void result.current.refreshCollectionModels('b'); });
  await act(async () => pending[3].resolve([{id: 'b1'}, {id: 'b2'}]));
  await act(async () => pending[2].resolve([]));
  expect(result.current.collectionModels).toEqual([{id: 'b1'}, {id: 'b2'}]);
});

it('waits for member refresh after a drop and retains the active members meanwhile', async () => {
  let refresh!: (files: unknown[]) => void;
  let added = false;
  vi.mocked(invoke).mockImplementation(command => {
    if (command === 'add_files_to_collection') { added = true; return Promise.resolve(undefined); }
    if (command === 'list_collection_files') return added
      ? new Promise(resolve => { refresh = resolve; }) : Promise.resolve([{id: 'm1'}]);
    return Promise.resolve([]);
  });
  const { result } = renderHook(() => useCollections());
  act(() => result.current.setActiveCollection('c1'));
  await waitFor(() => expect(result.current.collectionModels).toEqual([{id: 'm1'}]));
  let finished = false;
  await act(async () => { void result.current.addModelToCollection('m2', 'c1').then(() => { finished = true; }); });
  expect(finished).toBe(false);
  expect(result.current.collectionModels).toEqual([{id: 'm1'}]);
  await act(async () => refresh([{id: 'm1'}, {id: 'm2'}]));
  expect(finished).toBe(true);
  expect(result.current.activeCollection).toBe('c1');
  expect(result.current.collectionModels).toEqual([{id: 'm1'}, {id: 'm2'}]);
});

it('retains the collection order and persists manual reordering', async () => {
  vi.mocked(invoke).mockImplementation(command => Promise.resolve(command === 'list_collection_files'
    ? [{id: 'z'}, {id: 'a'}] : []));
  const { result } = renderHook(() => useCollections());
  act(() => result.current.setActiveCollection('c1'));
  await waitFor(() => expect(result.current.collectionModels.map(model => model.id)).toEqual(['z', 'a']));
  act(() => result.current.reorderCollection(['a', 'z']));
  expect(result.current.collectionModels.map(model => model.id)).toEqual(['a', 'z']);
  expect(invoke).toHaveBeenCalledWith('reorder_collection', {collectionId: 'c1', updates: [
    {fileId: 'a', position: 0}, {fileId: 'z', position: 1},
  ]});
});
