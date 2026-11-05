import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import * as api from './fileDrag';

beforeEach(() => invoke.mockReset().mockResolvedValue(undefined));

describe('fileDrag api', () => {
  it('reads and sets the switch', async () => {
    invoke.mockResolvedValue(false);
    expect(await api.getDragOutEnabled()).toBe(false);
    expect(invoke).toHaveBeenCalledWith('get_drag_out_enabled');
    await api.setDragOutEnabled(true);
    expect(invoke).toHaveBeenCalledWith('set_drag_out_enabled', { enabled: true });
  });
  it('starts a drag with ids only', async () => {
    await api.startFileDrag(['3', '5']);
    expect(invoke).toHaveBeenCalledWith('start_file_drag', { fileIds: ['3', '5'] });
  });
});
