import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { KeyboardTipsDialog, EmptyCatalogTips } from './KeyboardTipsDialog';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
afterEach(() => vi.unstubAllGlobals());
it('switches tabs with arrows, Home and End and exposes linked panels', () => {
  render(<LanguageProvider><KeyboardTipsDialog onClose={vi.fn()} /></LanguageProvider>);
  const keys = screen.getByRole('tab', { name: 'Tastenkürzel' }); const tips = screen.getByRole('tab', { name: 'Tipps' });
  expect(keys).toHaveFocus(); fireEvent.keyDown(keys, { key: 'ArrowRight' }); expect(tips).toHaveFocus(); expect(tips).toHaveAttribute('aria-selected', 'true');
  expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby', tips.id);
  fireEvent.keyDown(tips, { key: 'ArrowRight' }); expect(keys).toHaveFocus();
  fireEvent.keyDown(keys, { key: 'End' }); expect(tips).toHaveFocus(); fireEvent.keyDown(tips, { key: 'Home' }); expect(keys).toHaveFocus();
});
it('traps focus, closes with Escape and returns focus to the opener', () => {
  const opener = document.createElement('button'); document.body.append(opener); opener.focus(); const close = vi.fn();
  const result = render(<LanguageProvider><KeyboardTipsDialog onClose={close} /></LanguageProvider>);
  const first = screen.getByRole('tab', { name: 'Tastenkürzel' }); const last = screen.getByRole('button', { name: 'Schließen' });
  fireEvent.keyDown(first, { key: 'Tab', shiftKey: true }); expect(last).toHaveFocus(); fireEvent.keyDown(last, { key: 'Tab' }); expect(first).toHaveFocus();
  opener.focus(); expect(first).toHaveFocus(); fireEvent.keyDown(first, { key: 'Escape' }); expect(close).toHaveBeenCalledOnce();
  result.unmount(); expect(opener).toHaveFocus(); opener.remove();
});
it('shows Cmd from userAgentData on macOS', () => {
  vi.stubGlobal('navigator', { userAgentData: { platform: 'macOS' }, platform: 'Linux' });
  render(<LanguageProvider><KeyboardTipsDialog onClose={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText('Cmd+A')).toBeVisible(); expect(screen.getByText('Cmd+F')).toBeVisible(); expect(screen.queryByText('Strg+A')).toBeNull();
});
it('shows the empty catalog hint cards and opens help', () => {
  const open = vi.fn(); render(<LanguageProvider><EmptyCatalogTips onOpenTips={open} /></LanguageProvider>);
  for (const name of ['Auswählen', 'Rechtsklick', 'Schnell suchen']) expect(screen.getByRole('heading', { name })).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Tastenkürzel und Tipps' })); expect(open).toHaveBeenCalledOnce();
});
