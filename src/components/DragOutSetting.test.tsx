import { describe, expect, it, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { de } from '../i18n/de';

const getDragOutEnabled = vi.fn();
const setDragOutEnabled = vi.fn();
vi.mock('../lib/api/fileDrag', () => ({
  getDragOutEnabled: () => getDragOutEnabled(),
  setDragOutEnabled: (v: boolean) => setDragOutEnabled(v),
}));

import { DragOutSetting } from './DragOutSetting';

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); getDragOutEnabled.mockReset().mockResolvedValue(false); setDragOutEnabled.mockReset().mockResolvedValue(undefined); });

describe('DragOutSetting', () => {
  it('shows that it is loading instead of nothing, with the switch not yet usable', async () => {
    getDragOutEnabled.mockReturnValue(new Promise(() => {}));
    render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    expect(screen.getByRole('status')).toHaveTextContent(de.settingLoading);
    expect(screen.getByRole('switch', { name: de.dgSwitchLabel })).toBeDisabled();
  });

  it('reports a failed read with a way to read again', async () => {
    getDragOutEnabled.mockRejectedValueOnce(new Error('boom')).mockResolvedValue(true);
    render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    expect(await screen.findByRole('alert')).toHaveTextContent(de.settingLoadFailed);
    fireEvent.click(screen.getByRole('button', { name: de.settingReload }));
    await waitFor(() => expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'true'));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('allows one write at a time and shows the progress', async () => {
    let finish: () => void = () => {};
    setDragOutEnabled.mockReturnValue(new Promise<void>(resolve => { finish = resolve; }));
    render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    const toggle = await screen.findByRole('switch', { name: de.dgSwitchLabel });
    fireEvent.click(toggle);
    expect(await screen.findByRole('status')).toHaveTextContent(de.settingSaving);
    expect(toggle).toBeDisabled();
    fireEvent.click(toggle);
    expect(setDragOutEnabled).toHaveBeenCalledTimes(1);
    finish();
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).toHaveAttribute('aria-checked', 'true');
  });

  it('keeps the previous value and says so when saving fails', async () => {
    setDragOutEnabled.mockRejectedValue(new Error('boom'));
    render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    const toggle = await screen.findByRole('switch', { name: de.dgSwitchLabel });
    fireEvent.click(toggle);
    expect(await screen.findByRole('alert')).toHaveTextContent(de.settingSaveFailedKept);
    expect(toggle).toHaveAttribute('aria-checked', 'false');
  });

  it('is off by default and turns on through the backend', async () => {
    render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    const toggle = await screen.findByRole('switch', { name: de.dgSwitchLabel });
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
    expect(setDragOutEnabled).toHaveBeenCalledWith(true);
  });
});
