import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ImportResultDialog } from './ImportResultDialog';
import { importResult } from '../test/importFixtures';
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
const setup = () => render(<LanguageProvider><ImportResultDialog result={importResult} onClose={vi.fn()} /></LanguageProvider>);
it('switches tabs by click and keyboard and indents archive models', () => {
  setup(); expect(screen.getByText('Leere Datei (0 Byte)')).toBeVisible();
  const tabs = screen.getAllByRole('tab'); fireEvent.click(tabs[1]); expect(screen.getByText(/Gleicher Inhalt/)).toBeVisible();
  fireEvent.keyDown(tabs[1],{key:'ArrowRight'}); expect(tabs[2]).toHaveFocus(); expect(screen.getByText(/Importiert, aber nicht abgelegt/)).toBeVisible();
  fireEvent.keyDown(tabs[2],{key:'End'}); expect(tabs[3]).toHaveFocus(); expect(screen.getByText('Nicht entpackt: im Archivdialog abgebrochen')).toBeVisible();
  expect(screen.getByText(/bad.stl/).closest('tr')).toHaveClass('import-nested');
  fireEvent.keyDown(tabs[3],{key:'ArrowRight'}); expect(tabs[0]).toHaveFocus();
  fireEvent.keyDown(tabs[0],{key:'ArrowLeft'}); expect(tabs[3]).toHaveFocus();
});
it('copies every group and nested paths, with feedback', async () => {
  const writeText = vi.fn().mockResolvedValue(undefined); Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText}}); setup();
  fireEvent.click(screen.getByRole('button',{name:'Liste kopieren'})); await waitFor(() => expect(screen.getByText('kopiert')).toBeVisible());
  expect(writeText).toHaveBeenCalledWith(expect.stringContaining('Übersprungen\n/leer.stl\tLeere Datei (0 Byte)'));
  expect(writeText).toHaveBeenCalledWith(expect.stringContaining('\n  ../bad.stl\tUnsicherer Pfad, übersprungen'));
  expect(writeText).toHaveBeenCalledWith(expect.stringContaining('/alt.zip\tNicht entpackt: im Archivdialog abgebrochen'));
});
it.each([undefined, {writeText: () => Promise.reject(new Error('denied'))}])('offers selectable text when clipboard is unavailable', async clipboard => {
  Object.defineProperty(navigator,'clipboard',{configurable:true,value:clipboard}); setup(); fireEvent.click(screen.getByRole('button',{name:'Liste kopieren'}));
  expect((await screen.findByRole('textbox') as HTMLTextAreaElement).value).toContain('/leer.stl');
});
it.each([['en','Empty file (0 bytes)'],['es','Archivo vacío (0 bytes)'],['fr','Fichier vide (0 octet)']])('translates reasons in %s', (language, reason) => {
  localStorage.setItem('3mf-katalog-language',language); setup(); expect(screen.getByText(reason)).toBeVisible();
});

it.each([0, 1, 2, 3, -1])('opens the first populated tab (%s)', first => {
  const groups = { ...importResult.groups, skipped: [], duplicate: [], importedNotPlaced: [], archive: [] };
  const keys = ['skipped', 'duplicate', 'importedNotPlaced', 'archive'] as const;
  if (first >= 0) Object.assign(groups, { [keys[first]]: importResult.groups[keys[first]] });
  render(<LanguageProvider><ImportResultDialog result={{...importResult, groups}} onClose={vi.fn()} /></LanguageProvider>);
  expect(screen.getAllByRole('tab')[Math.max(0, first)]).toHaveAttribute('aria-selected', 'true');
});

it.each([['de', 'Archiv 1'], ['en', 'archive 1'], ['es', 'archivo comprimido 1'], ['fr', 'archive 1']])('uses the archive singular in %s', (language, name) => {
  localStorage.setItem('3mf-katalog-language', language);
  const result = {...importResult, groups: {...importResult.groups, archive: importResult.groups.archive.slice(0, 1)}};
  render(<LanguageProvider><ImportResultDialog result={result} onClose={vi.fn()} /></LanguageProvider>);
  expect(screen.getByRole('tab', {name})).toBeVisible();
});
