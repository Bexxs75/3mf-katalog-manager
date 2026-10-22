import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DateRenameBanner } from './DateRenameBanner';

const renderBanner = (outcome: { renamed: number; failed: number } | null, onClose: () => void) =>
  render(<LanguageProvider><DateRenameBanner outcome={outcome} onClose={onClose} /></LanguageProvider>);

describe('DateRenameBanner auto hide', () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('closes itself after five seconds when nothing failed', () => {
    const onClose = vi.fn();
    renderBanner({ renamed: 1, failed: 0 }, onClose);
    act(() => { vi.advanceTimersByTime(4900); });
    expect(onClose).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(200); });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('stays while the pointer is over it', () => {
    const onClose = vi.fn();
    renderBanner({ renamed: 1, failed: 0 }, onClose);
    fireEvent.mouseEnter(screen.getByRole('status'));
    act(() => { vi.advanceTimersByTime(20000); });
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.mouseLeave(screen.getByRole('status'));
    act(() => { vi.advanceTimersByTime(5100); });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('stays when a rename failed', () => {
    const onClose = vi.fn();
    renderBanner({ renamed: 1, failed: 1 }, onClose);
    act(() => { vi.advanceTimersByTime(20000); });
    expect(onClose).not.toHaveBeenCalled();
  });
});
