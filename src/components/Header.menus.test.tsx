import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { Header } from './Header';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
const props = { view: 'grid' as const, sort: 'name' as const, sortDirection: 'asc' as const, onSortDirectionChange: vi.fn(), mainView: 'catalog' as const,
  count: 2, onViewChange: vi.fn(), onSortChange: vi.fn(), onImportFiles: vi.fn(), onImportFolder: vi.fn(), allFoldersCollapsed: false, onToggleAllFolders: vi.fn() };
function setup() {
  return render(<LanguageProvider><Header {...props} /></LanguageProvider>);
}
const sortButton = () => screen.getByRole('button', { name: /Name/ });
const importButton = () => screen.getByRole('button', { name: /Importieren/ });
describe('Header menus', () => {
  it.each(['mouseDown', 'click'] as const)('dismisses sort on outside %s', (event) => {
    setup(); fireEvent.click(sortButton());
    expect(screen.getByRole('menuitemradio', { name: 'Dateigröße' })).toBeInTheDocument();
    fireEvent[event](document.body);
    expect(screen.queryByRole('menuitemradio', { name: 'Dateigröße' })).not.toBeInTheDocument();
  });
  it('Escape dismisses only the menu and restores trigger focus', () => {
    setup(); const trigger = sortButton(); fireEvent.click(trigger);
    screen.getByRole('menuitemradio', { name: 'Dateigröße' }).focus();
    const appEscape = vi.fn(); window.addEventListener('keydown', appEscape);
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    window.removeEventListener('keydown', appEscape);
    expect(appEscape).not.toHaveBeenCalled(); expect(trigger).toHaveFocus();
    expect(screen.queryByRole('menuitemradio', { name: 'Dateigröße' })).not.toBeInTheDocument();
  });
  it('opens only one menu and dismisses import outside and on Escape', () => {
    setup(); fireEvent.click(sortButton()); fireEvent.click(importButton());
    expect(screen.queryByRole('menuitemradio', { name: 'Dateigröße' })).not.toBeInTheDocument();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole('button', { name: 'Dateien...' })).not.toBeInTheDocument();
    const trigger = importButton(); fireEvent.click(trigger); fireEvent.keyDown(window, { key: 'Escape' });
    expect(trigger).toHaveFocus();
    fireEvent.click(trigger); fireEvent.click(sortButton());
    expect(screen.queryByRole('button', { name: 'Dateien...' })).not.toBeInTheDocument();
  });
  it('keeps sort open after selecting and closes after import selection', () => {
    setup(); fireEvent.click(sortButton()); fireEvent.click(screen.getByRole('menuitemradio', { name: 'Dateigröße' }));
    expect(props.onSortChange).toHaveBeenCalledWith('size', 'desc');
    expect(screen.getByRole('menuitemradio', { name: 'Dateigröße' })).toBeInTheDocument();
    fireEvent.click(importButton()); fireEvent.click(screen.getByRole('button', { name: 'Dateien...' }));
    expect(props.onImportFiles).toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Dateien...' })).not.toBeInTheDocument();
  });
  it('closes menus on detail opening and main view changes', () => {
    const result = setup(); fireEvent.click(sortButton());
    result.rerender(<LanguageProvider><Header {...props} detailOpen /></LanguageProvider>);
    expect(screen.queryByRole('menuitemradio', { name: 'Dateigröße' })).not.toBeInTheDocument();
    fireEvent.click(importButton());
    result.rerender(<LanguageProvider><Header {...props} mainView="trash" /></LanguageProvider>);
    result.rerender(<LanguageProvider><Header {...props} /></LanguageProvider>);
    expect(screen.queryByRole('button', { name: 'Dateien...' })).not.toBeInTheDocument();
  });
});

it.each([
  ['name', 'Name', 'asc', 'A → Z', 'Z → A'],
  ['imported', 'Importdatum', 'desc', 'Älteste zuerst', 'Neueste zuerst'],
  ['modified', 'Änderungsdatum neu', 'desc', 'Älteste zuerst', 'Neueste zuerst'],
  ['size', 'Dateigröße', 'desc', 'Kleinste zuerst', 'Größte zuerst'],
  ['vol', 'Volumen', 'desc', 'Kleinstes zuerst', 'Größtes zuerst'],
  ['viewed', 'Zuletzt angesehen', 'desc', 'Älteste zuerst', 'Neueste zuerst'],
] as const)('selects %s with its default and exposes direction controls', (sort, label, direction, asc, desc) => {
  const onSortChange = vi.fn(); const onSortDirectionChange = vi.fn();
  const result = render(<LanguageProvider><Header {...props} onSortChange={onSortChange} onSortDirectionChange={onSortDirectionChange} /></LanguageProvider>);
  fireEvent.click(sortButton());
  expect(screen.getAllByRole('menuitemradio')).toHaveLength(6);
  expect(screen.getByRole('group', { name: 'Richtung' })).toBeVisible();
  fireEvent.click(screen.getByRole('menuitemradio', { name: sort === 'modified' ? /Änderungsdatum\s*neu/ : label }));
  expect(onSortChange).toHaveBeenCalledWith(sort, direction);
  result.rerender(<LanguageProvider><Header {...props} sort={sort} sortDirection={direction} onSortChange={onSortChange} onSortDirectionChange={onSortDirectionChange} /></LanguageProvider>);
  expect(screen.getByRole('menuitemradio', { name: sort === 'modified' ? /Änderungsdatum\s*neu/ : label })).toHaveAttribute('aria-checked', 'true');
  expect(screen.getByRole('button', { name: `${sort === 'modified' ? 'Änderungsdatum' : label}${direction === 'asc' ? '↑' : '↓'}▾` })).toBeVisible();
  expect(screen.getByRole('button', { name: `${direction === 'asc' ? '↑' : '↓'} ${direction === 'asc' ? asc : desc}` })).toHaveAttribute('aria-pressed', 'true');
  onSortChange.mockClear();
  fireEvent.click(screen.getByRole('button', { name: `↑ ${asc}` }));
  expect(onSortDirectionChange).toHaveBeenCalledWith('asc');
  fireEvent.click(screen.getByRole('button', { name: `↓ ${desc}` }));
  expect(onSortDirectionChange).toHaveBeenLastCalledWith('desc');
  expect(onSortChange).not.toHaveBeenCalled();
  expect(screen.getByRole('menu')).toBeVisible();
});

it('closes the sort menu when the catalog view changes', () => {
  const result = setup(); fireEvent.click(sortButton());
  result.rerender(<LanguageProvider><Header {...props} view="groupedGrid" /></LanguageProvider>);
  expect(screen.queryByRole('menu')).not.toBeInTheDocument();
});
