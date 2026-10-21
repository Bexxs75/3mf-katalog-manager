import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ImportProgressRow } from './ImportProgressRow';
import { importProgress, importResult } from '../test/importFixtures';
import type { ImportCounts, ImportState } from '../types';
vi.mock('../diagnostics/DiagnosticsContext', () => ({useDiagnostics: () => ({openBugReport: vi.fn()})}));
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
it.each([['scanning','Suche'],['importing','Importiert'],['placing','Ablage'],['finished','Fertig, mit Hinweisen'],['cancelled','Abgebrochen'],['failed','Fehler']] as const)('renders %s with no percent text', (state, chip) => {
  const end = ['finished','cancelled','failed'].includes(state);
  render(<LanguageProvider><ImportProgressRow progress={{...importProgress, state}} meta={{jobId:'j1',source:'files',targetName:'Küche'}} result={end ? {...importResult, state: state as 'finished'} : null} queued={1} onCancel={vi.fn()} onDismiss={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText(chip)).toBeVisible(); expect(screen.getByText('· 1 Auftrag wartet')).toBeVisible();
  expect(screen.getByText('292')).toBeVisible(); expect(screen.queryByText(/\d+%/)).toBeNull();
  if (state === 'scanning') expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
  else expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow','310');
  if (state === 'failed') expect(screen.getByRole('button',{name:'Problem melden'})).toBeVisible();
});
it('disables cancel immediately while the request is unresolved', () => {
  const cancel = vi.fn(() => new Promise<void>(() => {}));
  render(<LanguageProvider><ImportProgressRow progress={importProgress} meta={{jobId:'j1',source:'dropped'}} result={null} queued={0} onCancel={cancel} onDismiss={vi.fn()} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button',{name:'Abbrechen'}));
  expect(screen.getByRole('button',{name:'Wird abgebrochen …'})).toBeDisabled(); expect(cancel).toHaveBeenCalledOnce();
});
it('dismisses on close and returns focus from details on Escape', () => {
  function Harness() { const [open,setOpen] = useState(true); return open ? <ImportProgressRow progress={{...importProgress,state:'finished' as ImportState}} meta={{jobId:'j1',source:'files'}} result={importResult} queued={0} onCancel={vi.fn()} onDismiss={() => setOpen(false)} /> : null; }
  render(<LanguageProvider><Harness /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button',{name:'Details'})); expect(screen.getByRole('dialog')).toBeVisible();
  fireEvent.keyDown(document,{key:'Escape'}); expect(screen.queryByRole('dialog')).toBeNull(); expect(screen.getByRole('button',{name:'Details'})).toHaveFocus();
  fireEvent.click(screen.getByRole('button',{name:'Schließen'})); expect(screen.queryByRole('status')).toBeNull();
});
it('shows scan discoveries and the current folder independently of processed counts', () => {
  render(<LanguageProvider><ImportProgressRow progress={{...importProgress,state:'scanning',total:null,found:217,current:'/models/kitchen',counts:{...importProgress.counts,known:0}}} meta={{jobId:'j1',source:'dropped'}} result={null} queued={0} onCancel={vi.fn()} onDismiss={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText('217 Dateien gefunden … · /models/kitchen')).toBeVisible(); expect(screen.getByRole('progressbar')).not.toHaveAttribute('aria-valuenow');
});
it('reports an incomplete scan on job failure as well as cancellation', () => {
  render(<LanguageProvider><ImportProgressRow progress={{...importProgress,state:'failed'}} meta={{jobId:'j1',source:'files'}} result={{...importResult,state:'failed',scanComplete:false,jobError:{kind:'database',message:'Datenbank nicht erreichbar'}}} queued={0} onCancel={vi.fn()} onDismiss={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText(/Suche abgebrochen nach 313 Dateien/)).toBeVisible();
});

const renderFinished = (counts: Partial<ImportCounts>) => render(<LanguageProvider><ImportProgressRow progress={{...importProgress,state:'finished' as ImportState}} meta={{jobId:'j1',source:'files'}} result={{...importResult,counts:{...importResult.counts,imported:0,importedNotPlaced:0,...counts},groups:{...importResult.groups,archive:[]}}} queued={0} onCancel={vi.fn()} onDismiss={vi.fn()} /></LanguageProvider>);
it('counts models imported but not placed as imported', () => {
  renderFinished({imported: 0, importedNotPlaced: 2});
  expect(screen.getByText(/2 Modelle importiert/)).toBeVisible();
  expect(screen.getAllByText('2', {selector: 'b'})).toHaveLength(2);
  expect(screen.getByText('davon nicht abgelegt', {exact: false})).toBeVisible();
});
it('adds placed and unplaced models in a mixed result', () => {
  renderFinished({imported: 3, importedNotPlaced: 2});
  expect(screen.getByText(/5 Modelle importiert/)).toBeVisible();
});
it('shows zero models and no unplaced entry when nothing was imported', () => {
  renderFinished({});
  expect(screen.getByText(/0 Modelle importiert/)).toBeVisible();
  expect(screen.queryByText(/davon nicht abgelegt/)).toBeNull();
});
it('uses the singular for one model and keeps the plain count without unplaced models', () => {
  renderFinished({imported: 1});
  expect(screen.getByText(/1 Modell importiert/)).toBeVisible();
  expect(screen.queryByText(/davon nicht abgelegt/)).toBeNull();
});

it.each([0, 1])('shows skipped archives with %s imported models', (imported) => {
  render(<LanguageProvider><ImportProgressRow progress={importProgress} meta={{jobId:'j1',source:'folderContents'}} result={{...importResult, archivesSkipped:1, counts:{...importResult.counts,imported,importedNotPlaced:0}, groups:{...importResult.groups,archive:[]}}} queued={0} onCancel={vi.fn()} onDismiss={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText('1 Archiv im Ordner wurde nicht importiert. Archive importierst du über „Importieren → Dateien...“.')).toBeVisible();
});
it('does not show the archive hint when no archives were skipped', () => {
  renderFinished({imported: 0});
  expect(screen.queryByText(/Archiv.*im Ordner/)).toBeNull();
});

it.each([
  ['de', 2, '2 Archive im Ordner wurden nicht importiert. Archive importierst du über „Importieren → Dateien...“.'],
  ['en', 1, '1 archive in the folder was not imported. Import archives via “Import → Files...”.'],
  ['en', 2, '2 archives in the folder were not imported. Import archives via “Import → Files...”.'],
  ['es', 1, 'No se ha importado 1 archivo comprimido de la carpeta. Importa archivos comprimidos mediante «Importar → Archivos...».'],
  ['es', 2, 'No se han importado 2 archivos comprimidos de la carpeta. Importa archivos comprimidos mediante «Importar → Archivos...».'],
  ['fr', 1, '1 archive du dossier n’a pas été importée. Importe les archives via «Importer → Fichiers...».'],
  ['fr', 2, '2 archives du dossier n’ont pas été importées. Importe les archives via «Importer → Fichiers...».'],
] as const)('renders the archive hint in %s for %s archives from terminal progress', (language, archivesSkipped, message) => {
  localStorage.setItem('3mf-katalog-language', language);
  render(<LanguageProvider><ImportProgressRow progress={{...importProgress,state:'finished',archivesSkipped}} meta={{jobId:'j1',source:'folder'}} result={null} queued={0} onCancel={vi.fn()} onDismiss={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText(message)).toBeVisible();
});
it('waits until the import finishes before showing the archive hint', () => {
  render(<LanguageProvider><ImportProgressRow progress={{...importProgress,archivesSkipped:1}} meta={{jobId:'j1',source:'folderContents'}} result={null} queued={0} onCancel={vi.fn()} onDismiss={vi.fn()} /></LanguageProvider>);
  expect(screen.queryByText(/Archiv.*im Ordner/)).toBeNull();
});


it('exposes bidi paths embedded in import failure messages without changing the result', () => {
  const jobError = Object.freeze({kind: 'database' as const, message: 'Cannot import /models/part\u202etxt.3mf'});
  render(<LanguageProvider><ImportProgressRow progress={{...importProgress, state: 'failed'}} meta={{jobId: 'j1', source: 'files'}}
    result={{...importResult, state: 'failed', jobError}} queued={0} onCancel={vi.fn()} onDismiss={vi.fn()} /></LanguageProvider>);
  expect(screen.getByText(/Cannot import \/models\/part\[U\+202E\]txt\.3mf/)).toBeVisible();
  expect(jobError.message).toBe('Cannot import /models/part\u202etxt.3mf');
});

it('keeps the green done state and plain label when nothing was skipped or left unplaced', () => {
  renderFinished({skipped: 0, importedNotPlaced: 0, imported: 4});
  expect(screen.getByText('Fertig')).toBeVisible();
  expect(screen.getByRole('status')).toHaveClass('done'); expect(screen.getByRole('status')).not.toHaveClass('warn-done');
});
it('shows a warning state with its own label when files were skipped', () => {
  renderFinished({skipped: 3, importedNotPlaced: 0, imported: 4});
  expect(screen.getByText('Fertig, mit Hinweisen')).toBeVisible();
  expect(screen.getByRole('status')).toHaveClass('warn-done'); expect(screen.getByRole('status')).not.toHaveClass('done');
});
it('shows a warning state when models were imported but not placed', () => {
  renderFinished({skipped: 0, importedNotPlaced: 2, imported: 4});
  expect(screen.getByText('Fertig, mit Hinweisen')).toBeVisible();
  expect(screen.getByRole('status')).toHaveClass('warn-done'); expect(screen.getByRole('status')).not.toHaveClass('done');
});
it('offers "view recently added" after an import that added models, closes the notice and calls back', () => {
  const onShowNew = vi.fn(); const onDismiss = vi.fn();
  render(<LanguageProvider><ImportProgressRow progress={{...importProgress,state:'finished' as ImportState}} meta={{jobId:'j1',source:'files'}} result={importResult} queued={0} onCancel={vi.fn()} onDismiss={onDismiss} onShowNew={onShowNew} /></LanguageProvider>);
  const button = screen.getByRole('button',{name:'Neu hinzugefügt ansehen'});
  button.focus(); expect(button).toHaveFocus();
  fireEvent.click(button);
  expect(onShowNew).toHaveBeenCalledOnce(); expect(onDismiss).toHaveBeenCalledOnce();
});
it('does not offer "view recently added" when nothing new was imported', () => {
  const none: ImportCounts = {...importResult.counts, imported: 0, importedNotPlaced: 0};
  render(<LanguageProvider><ImportProgressRow progress={{...importProgress,state:'finished' as ImportState,counts:none}} meta={{jobId:'j1',source:'files'}} result={{...importResult,counts:none,groups:{...importResult.groups,archive:[]}}} queued={0} onCancel={vi.fn()} onDismiss={vi.fn()} onShowNew={vi.fn()} /></LanguageProvider>);
  expect(screen.queryByRole('button',{name:'Neu hinzugefügt ansehen'})).toBeNull();
});
