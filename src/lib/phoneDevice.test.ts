import { describe, expect, it } from 'vitest';
import { isPhoneBrowser } from './phoneDevice';

describe('isPhoneBrowser', () => {
  it.each([
    'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148',
    'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/126.0 Mobile Safari/537.36',
    'Mozilla/5.0 (Windows Phone 10.0; Android 6.0.1) Mobile Safari/537.36 Edge/15.15254',
  ])('detects phone %#', ua => expect(isPhoneBrowser(ua)).toBe(true));

  it.each([
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/17.0 Safari/605.1.15',
    'Mozilla/5.0 (Linux; Android 14; SM-X710) AppleWebKit/537.36 Chrome/126.0 Safari/537.36',
    'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/126.0 Safari/537.36',
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/126.0 Safari/537.36',
  ])('allows tablet and desktop %#', ua => expect(isPhoneBrowser(ua)).toBe(false));
});
