import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { consumeResin, readDroppedImage, restockFilamentSpool, formatTemperatureRange } from './filament';

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

describe('readDroppedImage', () => {
  it('calls read_dropped_image with the path', async () => {
    vi.mocked(invoke).mockResolvedValue('QUJD');
    await expect(readDroppedImage('/home/u/spule.png')).resolves.toBe('QUJD');
    expect(invoke).toHaveBeenCalledWith('read_dropped_image', { path: '/home/u/spule.png' });
  });
});

describe('formatTemperatureRange', () => {
  it.each([
    [200, 240, '200–240 °C'],
    [215, null, '215 °C'],
    [null, 215, '215 °C'],
    [215, 215, '215 °C'],
    [0, 500, '0–500 °C'],
    [null, null, ''],
  ])('formats %s / %s', (min, max, expected) => {
    expect(formatTemperatureRange(min, max)).toBe(expected);
  });
});
