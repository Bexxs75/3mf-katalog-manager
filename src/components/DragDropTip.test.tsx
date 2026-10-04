import { beforeEach, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { DragDropTip } from './DragDropTip';
const key = '3mf-katalog-dnd-tip-dismissed';
beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
it.each([[0, 1], [1, 0], [0, 0]])('needs models and folders (%s, %s)', (modelCount, folderCount) => {
  render(<LanguageProvider><DragDropTip modelCount={modelCount} folderCount={folderCount} /></LanguageProvider>);
  expect(screen.queryByText('Verstanden')).toBeNull();
});
it('dismisses persistently without taking focus', () => {
  const focus = document.activeElement;
  const { unmount } = render(<LanguageProvider><DragDropTip modelCount={1} folderCount={1} /></LanguageProvider>);
  expect(document.activeElement).toBe(focus);
  fireEvent.click(screen.getByText('Verstanden'));
  expect(localStorage.getItem(key)).toBe('true');
  expect(screen.queryByText('Verstanden')).toBeNull();
  unmount();
  render(<LanguageProvider><DragDropTip modelCount={1} folderCount={1} /></LanguageProvider>);
  expect(screen.queryByText('Verstanden')).toBeNull();
});
it('never appears with an existing key, including an empty value', () => {
  localStorage.setItem(key, '');
  render(<LanguageProvider><DragDropTip modelCount={1} folderCount={1} /></LanguageProvider>);
  expect(screen.queryByText('Verstanden')).toBeNull();
});

it('disappears immediately when a successful move dismisses it', async () => {
  const { act } = await import('@testing-library/react');
  const { dismissDragDropTip } = await import('../lib/dragDropTip');
  render(<LanguageProvider><DragDropTip modelCount={1} folderCount={1} /></LanguageProvider>);
  act(() => dismissDragDropTip());
  expect(screen.queryByText('Verstanden')).toBeNull();
});
