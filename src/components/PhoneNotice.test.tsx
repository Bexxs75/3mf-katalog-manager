import { afterEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PhoneNotice } from './PhoneNotice';

const PHONE = 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148';
const DESKTOP = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/126.0 Safari/537.36';

function setUa(ua: string) { Object.defineProperty(navigator, 'userAgent', { value: ua, configurable: true }); }
function view() { return render(<LanguageProvider><PhoneNotice><p>app</p></PhoneNotice></LanguageProvider>); }

afterEach(() => { sessionStorage.clear(); setUa(DESKTOP); });

describe('PhoneNotice', () => {
  it('passes through on desktop', () => {
    setUa(DESKTOP); view();
    expect(screen.getByText('app')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('shows the notice on a phone and continues on request', () => {
    setUa(PHONE); view();
    expect(screen.queryByText('app')).toBeNull();
    fireEvent.click(screen.getByRole('button'));
    expect(screen.getByText('app')).toBeTruthy();
  });

  it('stays dismissed for the rest of the session', () => {
    setUa(PHONE);
    const first = view(); fireEvent.click(screen.getByRole('button')); first.unmount();
    view();
    expect(screen.getByText('app')).toBeTruthy();
  });
});
