import { describe, expect, it } from 'vitest';
import { parseConsumeAmount } from './resinConsume';

describe('parseConsumeAmount', () => {
  it('reads decimals and rounds to 0.1', () => {
    expect(parseConsumeAmount('45')).toBe(45);
    expect(parseConsumeAmount('12,34')).toBe(12.3);
    expect(parseConsumeAmount('0.5')).toBe(0.5);
  });

  it('rejects empty, zero, too small and garbage', () => {
    for (const raw of ['', '0', '0,04', '-3', 'abc']) expect(parseConsumeAmount(raw)).toBeNull();
  });
});
