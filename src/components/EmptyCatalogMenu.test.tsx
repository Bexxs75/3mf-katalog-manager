import { useState } from 'react';
import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { EmptyCatalogMenu, type EmptyCatalogActions } from './EmptyCatalogMenu';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

beforeEach(() => { localStorage.clear(); localStorage.setItem('3mf-katalog-language', 'de'); });
function Harness({actions, busy = false}: {actions: EmptyCatalogActions; busy?: boolean}) {
  const [open, setOpen] = useState(false);
  return <><button onClick={event => { event.currentTarget.focus(); setOpen(true); }}>Open</button>
    {open && <EmptyCatalogMenu x={10} y={20} onClose={() => setOpen(false)} view="grid" sort="name" busy={busy} {...actions} />}</>;
}
const setup = (busy = false) => {
  const actions = {onSync: vi.fn(), onImport: vi.fn(), onNewFolder: vi.fn(), onSelectAll: vi.fn(), onViewChange: vi.fn(), onSortChange: vi.fn(), onSortDirectionChange: vi.fn()};
  render(<LanguageProvider><Harness actions={actions} busy={busy} /></LanguageProvider>);
  fireEvent.click(screen.getByText('Open')); return actions;
};
it('lists catalog actions and calls exactly the chosen existing handler', () => {
  const actions = setup();
  expect(screen.getAllByRole('menuitem').map(el => el.textContent)).toEqual([
    `${de.emptyMenuSync}F5`, de.emptyMenuImport, de.emptyMenuNewFolder, de.selectAllLabel, de.emptyMenuView, de.sortLabel,
  ]);
  for (const [name, fn] of [[de.emptyMenuImport, actions.onImport], [de.emptyMenuNewFolder, actions.onNewFolder], [de.selectAllLabel, actions.onSelectAll], [de.emptyMenuSync, actions.onSync]] as const) {
    fireEvent.click(screen.getByRole('menuitem', {name: new RegExp(name)})); expect(fn).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Open')).toHaveFocus(); fireEvent.click(screen.getByText('Open'));
  }
});
it('supports submenu arrows, checked values, Escape and focus return', () => {
  const actions = setup();
  const view = screen.getByRole('menuitem', {name: de.emptyMenuView});
  view.focus(); fireEvent.keyDown(view, {key: 'ArrowRight'});
  expect(screen.getByRole('menuitemradio', {name: de.viewGrid})).toHaveFocus();
  expect(screen.getByRole('menuitemradio', {name: de.viewGrid})).toHaveAttribute('aria-checked', 'true');
  fireEvent.keyDown(document.activeElement!, {key: 'ArrowDown'});
  expect(screen.getByRole('menuitemradio', {name: de.viewFolder})).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, {key: 'Escape'}); expect(view).toHaveFocus();
  fireEvent.keyDown(view, {key: 'ArrowRight'});
  fireEvent.click(screen.getByRole('menuitemradio', {name: de.viewList})); expect(actions.onViewChange).toHaveBeenCalledWith('groupedList');
  fireEvent.click(screen.getByText('Open'));
  fireEvent.click(screen.getByRole('menuitem', {name: de.sortLabel}));
  fireEvent.click(screen.getByRole('menuitemradio', {name: de.sortModified})); expect(actions.onSortChange).toHaveBeenCalledWith('modified');
  fireEvent.click(screen.getByText('Open')); fireEvent.click(screen.getByRole('menuitem', {name: de.sortLabel}));
  fireEvent.click(screen.getByRole('menuitemradio', {name: de.sortNameDesc})); expect(actions.onSortDirectionChange).toHaveBeenCalledWith('desc');
  fireEvent.click(screen.getByText('Open')); fireEvent.keyDown(document.activeElement!, {key: 'Escape'});
  expect(screen.queryByRole('menu')).toBeNull(); expect(screen.getByText('Open')).toHaveFocus();
});
it('runs F5 once and closes the menu, without passing the event to the global shortcut', () => {
  const actions = setup(); const global = vi.fn(); document.addEventListener('keydown', global);
  fireEvent.keyDown(document.activeElement!, {key: 'F5'});
  expect(actions.onSync).toHaveBeenCalledTimes(1); expect(global).not.toHaveBeenCalled();
  expect(screen.queryByRole('menu')).toBeNull(); document.removeEventListener('keydown', global);
});
it('disables file mutations during import but keeps view and selection available', () => {
  setup(true);
  for (const name of [de.emptyMenuSync, de.emptyMenuImport, de.emptyMenuNewFolder]) expect(screen.getByRole('menuitem', {name: new RegExp(name)})).toBeDisabled();
  expect(screen.getByRole('menuitem', {name: de.selectAllLabel})).toBeEnabled();
});
it('has translated labels and matching keyboard help in all languages', () => {
  for (const dict of [de, en, es, fr]) {
    for (const key of ['tagMenuFilter', 'tagMenuDelete', 'tagDeleteQuestion', 'tagDeleteHint', 'emptyMenuSync', 'emptyMenuImport', 'emptyMenuNewFolder', 'emptyMenuView', 'emptyMenuLabel'] as const) expect(dict[key].length).toBeGreaterThan(0);
    expect(dict.tipContextText).toContain('F10'); expect(dict.tagDeleteQuestion).toContain('{tag}');
  }
  localStorage.setItem('3mf-katalog-language', 'en'); setup();
  expect(screen.getByRole('menuitem', {name: en.emptyMenuImport})).toBeInTheDocument();
});
