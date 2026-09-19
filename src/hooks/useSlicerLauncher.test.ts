import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useSlicerLauncher } from './useSlicerLauncher';
import { makeModelFile } from '../test/factories';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
// Kein Ausdruckskoerper: vi.fn().mockReset() gibt die Mock-Funktion selbst
// zurueck, und Vitest behandelt einen von einem Hook zurueckgegebenen
// Funktionswert als automatisches Teardown, das nach dem Test erneut
// aufgerufen wird - bei einem mit mockRejectedValue belegten Mock fuehrt
// das zu einer scheinbar unbehandelten Ablehnung, die dem Test angelastet wird.
beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('useSlicerLauncher', () => {
  it('calls onNeedsSetup when there are no slicers configured', () => {
    const onNeedsSetup = vi.fn();
    const models = [makeModelFile({ id: 'm1' })];
    const { result } = renderHook(() => useSlicerLauncher(models, [], null, onNeedsSetup));
    act(() => result.current.openInSlicer('m1'));
    expect(onNeedsSetup).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('opens the primary slicer for the model path', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const models = [makeModelFile({ id: 'm1', path: '/model.3mf' })];
    const slicers = [{ id: 's1', name: 'Orca', path: '/usr/bin/orca', source: 'manual' as const }];
    const { result } = renderHook(() => useSlicerLauncher(models, slicers, 's1', vi.fn()));
    act(() => result.current.openInSlicer('m1'));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('open_in_slicer', { slicerPath: '/usr/bin/orca', filePath: '/model.3mf' }),
    );
  });

  it('sets slicerError when opening fails', async () => {
    vi.mocked(invoke).mockRejectedValue('not found');
    const models = [makeModelFile({ id: 'm1', path: '/model.3mf' })];
    const slicers = [{ id: 's1', name: 'Orca', path: '/usr/bin/orca', source: 'manual' as const }];
    const { result } = renderHook(() => useSlicerLauncher(models, slicers, 's1', vi.fn()));
    act(() => result.current.openInSlicer('m1'));
    await waitFor(() => expect(result.current.slicerError).toBe('not found'));
  });
});
