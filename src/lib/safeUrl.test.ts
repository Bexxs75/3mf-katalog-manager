import { describe, expect, it } from 'vitest';
import { isSafeHttpUrl } from './safeUrl';

describe('isSafeHttpUrl', () => {
  it('accepts http and https URLs', () => {
    expect(isSafeHttpUrl('http://example.org/x')).toBe(true);
    expect(isSafeHttpUrl('https://makerworld.com/de/models/1')).toBe(true);
  });

  it('rejects script-bearing and other non-http schemes', () => {
    expect(isSafeHttpUrl('javascript:alert(1)')).toBe(false);
    expect(isSafeHttpUrl('JavaScript:alert(1)')).toBe(false);
    expect(isSafeHttpUrl('data:text/html,<script>alert(1)</script>')).toBe(false);
    expect(isSafeHttpUrl('file:///etc/passwd')).toBe(false);
    expect(isSafeHttpUrl('tauri://localhost')).toBe(false);
  });

  it('rejects empty and missing values', () => {
    expect(isSafeHttpUrl('')).toBe(false);
    expect(isSafeHttpUrl(null)).toBe(false);
    expect(isSafeHttpUrl(undefined)).toBe(false);
  });
});
