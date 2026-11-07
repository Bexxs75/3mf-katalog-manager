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

const isWindowsPlatform = vi.fn(() => false);
vi.mock('../lib/imageDrop', () => ({ isWindowsPlatform: () => isWindowsPlatform() }));

import { DragOutSetting } from './DragOutSetting';

beforeEach(() => { isWindowsPlatform.mockReset().mockReturnValue(false); localStorage.setItem('3mf-katalog-language', 'de'); getDragOutEnabled.mockReset().mockResolvedValue(false); setDragOutEnabled.mockReset().mockResolvedValue(undefined); });

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

  it('is a section with a heading, the experimental badge and the general platform sentence', async () => {
    render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    const heading = await screen.findByRole('heading', { level: 3, name: new RegExp(de.dgSectionTitle) });
    expect(heading).toHaveTextContent(de.tvExperimental);
    expect(screen.getByText(de.dgSwitchHint)).toBeInTheDocument();
    expect(screen.getByText(de.dgExperimentalNote)).toBeInTheDocument();
  });

  it('shows no Windows advice on other systems', async () => {
    render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    const toggle = await screen.findByRole('switch', { name: de.dgSwitchLabel });
    expect(screen.queryByText(de.dgWinText)).toBeNull();
    expect(screen.queryByText(de.dgWinSummary)).toBeNull();
    expect(toggle.getAttribute('aria-describedby')).not.toContain('drag-out-windows');
  });

  it('shows the Windows advice and the guide even while the switch is off, and ties it to the switch', async () => {
    isWindowsPlatform.mockReturnValue(true);
    const { container } = render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    const toggle = await screen.findByRole('switch', { name: de.dgSwitchLabel });
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByText(de.dgWinText)).toBeVisible();
    expect(screen.getByText(de.dgWinSummary)).toBeVisible();
    expect(screen.getByText(de.dgWinStep1)).toBeInTheDocument();
    expect(container.querySelector('details')).not.toHaveAttribute('open');
    const described = toggle.getAttribute('aria-describedby')!.split(' ');
    expect(described).toContain('drag-out-windows');
    for (const id of described) expect(container.querySelector(`#${id}`)).not.toBeNull();
  });
});
