import { afterEach, expect, it, vi } from 'vitest';
import { act, render } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { SourceFolderNotice } from './SourceFolderNotice';
import type { SourceNotice } from '../hooks/useSourceFolderCheck';

afterEach(() => vi.useRealTimers());
it('expires the setup hint after six seconds without moving focus and restarts for a new notice', () => {
  vi.useFakeTimers();
  const close = vi.fn();
  const ui = (notice: SourceNotice) => <LanguageProvider><button autoFocus>Settings</button><SourceFolderNotice notice={notice} onView={vi.fn()} onClose={close} /></LanguageProvider>;
  const { rerender, getByText } = render(ui({ kind: 'noFolders' }));
  act(() => vi.advanceTimersByTime(5000));
  expect(close).not.toHaveBeenCalled();
  rerender(ui({ kind: 'noFolders' }));
  act(() => vi.advanceTimersByTime(5999));
  expect(close).not.toHaveBeenCalled();
  act(() => vi.advanceTimersByTime(1));
  expect(close).toHaveBeenCalledTimes(1);
  expect(getByText('Settings')).toHaveFocus();
});
it('keeps discovered files available until the user responds', () => {
  vi.useFakeTimers();
  const close = vi.fn();
  render(<LanguageProvider><SourceFolderNotice notice={{ kind: 'found', count: 1, folderCount: 1, folderPath: '/source', truncated: false, skipped: 0, unreachable: 0 }} onView={vi.fn()} onClose={close} /></LanguageProvider>);
  act(() => vi.advanceTimersByTime(60000));
  expect(close).not.toHaveBeenCalled();
});
