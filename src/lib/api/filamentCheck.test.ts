import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { checkFilament } from './filamentCheck';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('checkFilament', () => {
  it('calls check_filament with the ids in order', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await checkFilament(['3', '1']);
    expect(invoke).toHaveBeenCalledWith('check_filament', { fileIds: ['3', '1'] });
  });
});
