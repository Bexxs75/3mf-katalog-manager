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


it('shows bidi controls in unimported paths while retaining original paths in copied data', async () => {
  const path = '/models/part\u202etxt.3mf';
  const result = {...importResult, groups: {...importResult.groups, skipped: [{path, reason: 'empty' as const}]}};
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'clipboard', {configurable: true, value: {writeText}});
  render(<LanguageProvider><ImportResultDialog result={result} onClose={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText('/models/part[U+202E]txt.3mf')).toBeVisible();
  fireEvent.click(screen.getByRole('button', {name: 'Liste kopieren'}));
  await waitFor(() => expect(writeText).toHaveBeenCalledWith(expect.stringContaining(path)));
  expect(result.groups.skipped[0].path).toBe(path);
});

it.each([0, 2])('shows %s extracted non-model files in the archive group', otherFilesExtracted => {
  const archive = importResult.groups.archive[1];
  if (archive.state === 'pending') throw new Error('expected completed fixture');
  const result = {...importResult, groups: {...importResult.groups, archive: [{...archive, otherFilesExtracted}]}};
  render(<LanguageProvider><ImportResultDialog result={result} onClose={vi.fn()} /></LanguageProvider>);
  fireEvent.click(screen.getAllByRole('tab')[3]);
  if (otherFilesExtracted) expect(screen.getByText(/2 weitere Dateien wurden entpackt/)).toBeVisible();
  else expect(screen.queryByText(/weitere Dateien wurden entpackt/)).not.toBeInTheDocument();
});
it('lists assigned and ignored companion files as rows of the archive', () => {
  const archive = importResult.groups.archive[1];
  const result = {...importResult, groups: {...importResult.groups, archive: [{...archive, companionsAssigned: 3, companionsIgnored: 1, otherFilesExtracted: 1}]}};
  render(<LanguageProvider><ImportResultDialog result={result} onClose={vi.fn()} /></LanguageProvider>);
  fireEvent.click(screen.getAllByRole('tab')[3]);
  expect(screen.getByText('3 Begleitdateien wurden Modellen zugeordnet.')).toBeVisible();
  expect(screen.getByText(/1 Datei ignoriert \(nicht erlaubter Typ\)/)).toBeVisible();
  expect(screen.queryByText(/weitere Datei/)).toBeNull();
});

it('offers "view recently added" in the result dialog when models were added, closes the dialog and calls back', () => {
  const onShowNew = vi.fn(); const onClose = vi.fn();
  render(<LanguageProvider><ImportResultDialog result={importResult} onClose={onClose} onShowNew={onShowNew} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', {name: 'Neu hinzugefügt ansehen'}));
  expect(onShowNew).toHaveBeenCalledOnce(); expect(onClose).toHaveBeenCalledOnce();
});

it('hides "view recently added" in the result dialog when nothing new was imported or no handler is given', () => {
  const none = {...importResult.counts, imported: 0, importedNotPlaced: 0};
  const result = {...importResult, counts: none, groups: {...importResult.groups, importedNotPlaced: [], archive: []}};
  const {unmount} = render(<LanguageProvider><ImportResultDialog result={result} onClose={vi.fn()} onShowNew={vi.fn()} /></LanguageProvider>);
  expect(screen.queryByRole('button', {name: 'Neu hinzugefügt ansehen'})).toBeNull();
  unmount();
  render(<LanguageProvider><ImportResultDialog result={importResult} onClose={vi.fn()} /></LanguageProvider>);
  expect(screen.queryByRole('button', {name: 'Neu hinzugefügt ansehen'})).toBeNull();
});
