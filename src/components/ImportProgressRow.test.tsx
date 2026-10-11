import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ImportProgressRow } from './ImportProgressRow';
import { importProgress, importResult } from '../test/importFixtures';
import type { ImportCounts, ImportState } from '../types';
vi.mock('../diagnostics/DiagnosticsContext', () => ({useDiagnostics: () => ({openBugReport: vi.fn()})}));
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
it.each([['scanning','Suche'],['importing','Importiert'],['placing','Ablage'],['finished','Fertig'],['cancelled','Abgebrochen'],['failed','Fehler']] as const)('renders %s with no percent text', (state, chip) => {
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
