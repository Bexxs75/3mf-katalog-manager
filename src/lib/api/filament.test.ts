import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { consumeResin, restockFilamentSpool } from './filament';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('restockFilamentSpool', () => {
  it('calls restock_filament_spool with camelCase args', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await restockFilamentSpool('7', 3, 1000, 24.9, 'Regal 1');
    expect(invoke).toHaveBeenCalledWith('restock_filament_spool', {
      templateId: '7', count: 3, weight: 1000, price: 24.9, location: 'Regal 1',
    });
  });
});

describe('consumeResin', () => {
  it('calls consume_resin', async () => {
    vi.mocked(invoke).mockResolvedValue({});
    await consumeResin('9', 45.5);
    expect(invoke).toHaveBeenCalledWith('consume_resin', { spoolId: '9', amountMl: 45.5 });
  });
});
