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
  it('is off by default and turns on through the backend', async () => {
    render(<LanguageProvider><DragOutSetting /></LanguageProvider>);
    const toggle = await screen.findByRole('switch', { name: de.dgSwitchLabel });
    expect(toggle).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle).toHaveAttribute('aria-checked', 'true'));
    expect(setDragOutEnabled).toHaveBeenCalledWith(true);
  });
});
