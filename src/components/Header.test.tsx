import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Header } from './Header';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
const props = { view: 'grid' as const, sort: 'name' as const, mainView: 'catalog' as const,
  count: 2, onViewChange: vi.fn(), onSortChange: vi.fn(), onImportFiles: vi.fn(), onImportFolder: vi.fn() };
function setup() {
  return render(<LanguageProvider><Header {...props} /></LanguageProvider>);
}
const sortButton = () => screen.getByRole('button', { name: /Name/ });
const importButton = () => screen.getByRole('button', { name: /Importieren/ });
describe('Header menus', () => {
  it.each(['mouseDown', 'click'] as const)('dismisses sort on outside %s', (event) => {
    setup(); fireEvent.click(sortButton());
    expect(screen.getByRole('button', { name: 'Dateigröße' })).toBeInTheDocument();
    fireEvent[event](document.body);
    expect(screen.queryByRole('button', { name: 'Dateigröße' })).not.toBeInTheDocument();
  });
  it('Escape dismisses only the menu and restores trigger focus', () => {
    setup(); const trigger = sortButton(); fireEvent.click(trigger);
    screen.getByRole('button', { name: 'Dateigröße' }).focus();
    const appEscape = vi.fn(); window.addEventListener('keydown', appEscape);
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    window.removeEventListener('keydown', appEscape);
    expect(appEscape).not.toHaveBeenCalled(); expect(trigger).toHaveFocus();
    expect(screen.queryByRole('button', { name: 'Dateigröße' })).not.toBeInTheDocument();
  });
  it('opens only one menu and dismisses import outside and on Escape', () => {
    setup(); fireEvent.click(sortButton()); fireEvent.click(importButton());
    expect(screen.queryByRole('button', { name: 'Dateigröße' })).not.toBeInTheDocument();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole('button', { name: 'Dateien...' })).not.toBeInTheDocument();
    const trigger = importButton(); fireEvent.click(trigger); fireEvent.keyDown(window, { key: 'Escape' });
    expect(trigger).toHaveFocus();
    fireEvent.click(trigger); fireEvent.click(sortButton());
    expect(screen.queryByRole('button', { name: 'Dateien...' })).not.toBeInTheDocument();
  });
  it('closes after selecting sort or import options', () => {
    setup(); fireEvent.click(sortButton()); fireEvent.click(screen.getByRole('button', { name: 'Dateigröße' }));
    expect(props.onSortChange).toHaveBeenCalledWith('size');
    expect(screen.queryByRole('button', { name: 'Dateigröße' })).not.toBeInTheDocument();
    fireEvent.click(importButton()); fireEvent.click(screen.getByRole('button', { name: 'Dateien...' }));
    expect(props.onImportFiles).toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Dateien...' })).not.toBeInTheDocument();
  });
  it('closes menus on detail opening and main view changes', () => {
    const result = setup(); fireEvent.click(sortButton());
    result.rerender(<LanguageProvider><Header {...props} detailOpen /></LanguageProvider>);
    expect(screen.queryByRole('button', { name: 'Dateigröße' })).not.toBeInTheDocument();
    fireEvent.click(importButton());
    result.rerender(<LanguageProvider><Header {...props} mainView="trash" /></LanguageProvider>);
    result.rerender(<LanguageProvider><Header {...props} /></LanguageProvider>);
    expect(screen.queryByRole('button', { name: 'Dateien...' })).not.toBeInTheDocument();
  });
});
