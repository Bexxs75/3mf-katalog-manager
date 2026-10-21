import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { MoveToast } from './MoveToast';
import { RenameToast } from './RenameToast';
import type { MoveSummary } from '../lib/moveUndo';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

const summary = (overrides: Partial<MoveSummary> = {}): MoveSummary =>
  ({ moved: 0, skippedNameExists: 0, skippedAlreadyThere: 0, failed: 0, ...overrides });

function renderToast(props: Partial<Parameters<typeof MoveToast>[0]> = {}) {
  const onDone = vi.fn();
  const view = render(<LanguageProvider><MoveToast from="Halter.3mf" to="/katalog/Druckerteile" onDone={onDone} {...props} /></LanguageProvider>);
  return { onDone, ...view };
}

describe('move result toast', () => {
  it('names the folder and the count, lists the first two names and offers undo with its shortcut', () => {
    const onUndo = vi.fn();
    renderToast({ batch: { names: ['Halter.3mf', 'Clip.3mf'], total: 3, summary: summary({ moved: 3 }) }, onUndo });
    const toast = screen.getByRole('status');
    expect(toast).toHaveTextContent('3 Modelle nach „Druckerteile“ verschoben.');
    expect(toast).toHaveTextContent('Halter.3mf, Clip.3mf, …');
    expect(toast).toHaveTextContent('Strg+Z');
    fireEvent.click(screen.getByRole('button', { name: 'Rückgängig' }));
    expect(onUndo).toHaveBeenCalledTimes(1);
  });

  it('names the single file for one moved model', () => {
    renderToast({ batch: { names: ['Halter.3mf'], total: 1, summary: summary({ moved: 1 }) } });
    expect(screen.getByRole('status')).toHaveTextContent('„Halter.3mf“ nach „Druckerteile“ verschoben.');
  });

  it('says why files were skipped', () => {
    renderToast({ batch: { names: ['Halter.3mf'], total: 3, summary: summary({ moved: 2, skippedNameExists: 1 }) } });
    expect(screen.getByRole('status')).toHaveTextContent('2 verschoben, 1 übersprungen: gleicher Name im Ziel.');
    expect(screen.queryByRole('button', { name: 'Rückgängig' })).toBeNull();
  });

  it('says that files already in the target were skipped', () => {
    renderToast({ batch: { names: ['A'], total: 2, summary: summary({ moved: 1, skippedAlreadyThere: 1 }) } });
    expect(screen.getByRole('status')).toHaveTextContent('1 verschoben, 1 übersprungen: liegt schon im Zielordner.');
  });

  it('announces failures as an alert with the reason', () => {
    renderToast({ batch: { names: ['A', 'B'], total: 3, summary: summary({ moved: 2, failed: 1, firstFailReason: 'source_missing' }) } });
    const toast = screen.getByRole('alert');
    expect(toast).toHaveTextContent('2 verschoben, 1 fehlgeschlagen (Datei fehlt auf der Festplatte).');
  });

  it('leads with "Nichts verschoben" when nothing moved', () => {
    renderToast({ batch: { names: ['A'], total: 1, summary: summary({ skippedNameExists: 1 }) } });
    expect(screen.getByRole('status')).toHaveTextContent('Nichts verschoben, 1 übersprungen: gleicher Name im Ziel.');
  });

  it('stays 15 seconds when it can be undone and 3 seconds otherwise', () => {
    vi.useFakeTimers();
    try {
      const first = renderToast({ batch: { names: ['A'], total: 1, summary: summary({ moved: 1 }) }, onUndo: vi.fn() });
      vi.advanceTimersByTime(14999);
      expect(first.onDone).not.toHaveBeenCalled();
      vi.advanceTimersByTime(1);
      expect(first.onDone).toHaveBeenCalledTimes(1);
      first.unmount();
      const second = renderToast({ batch: { names: ['A'], total: 1, summary: summary({ moved: 1 }) } });
      vi.advanceTimersByTime(3000);
      expect(second.onDone).toHaveBeenCalledTimes(1);
    } finally { vi.useRealTimers(); }
  });

  it('offers undo on a folder move too', () => {
    const onUndo = vi.fn();
    renderToast({ onUndo });
    fireEvent.click(screen.getByRole('button', { name: 'Rückgängig' }));
    expect(onUndo).toHaveBeenCalled();
  });
});

describe('undo result toast', () => {
  it('confirms put-back files', () => {
    renderToast({ undo: { kind: 'files', ok: true, count: 3, total: 3 } });
    expect(screen.getByRole('status')).toHaveTextContent('Verschieben rückgängig: 3 Modelle zurückgelegt.');
  });

  it('confirms a folder move and a rename', () => {
    const { unmount } = renderToast({ undo: { kind: 'folderMove', ok: true, name: 'Deko' } });
    expect(screen.getByRole('status')).toHaveTextContent('Verschieben von „Deko“ rückgängig gemacht.');
    unmount();
    renderToast({ undo: { kind: 'rename', ok: true, name: 'Druckerteile', restoredName: 'Parts' } });
    expect(screen.getByRole('status')).toHaveTextContent('Umbenennen rückgängig: „Druckerteile“ heißt wieder „Parts“.');
  });

  it('says "Rückgängig nicht möglich" with a translated reason as alert', () => {
    renderToast({ undo: { kind: 'files', ok: false, count: 0, total: 2, reason: 'name_exists' }, error: true });
    expect(screen.getByRole('alert')).toHaveTextContent('Rückgängig nicht möglich: am alten Platz gibt es schon eine Datei mit diesem Namen');
  });

  it('shows a backend message verbatim and a partial result', () => {
    const { unmount } = renderToast({ undo: { kind: 'rename', ok: false, name: 'X', reason: 'Der Ordner existiert nicht mehr.' }, error: true });
    expect(screen.getByRole('alert')).toHaveTextContent('Rückgängig nicht möglich: Der Ordner existiert nicht mehr.');
    unmount();
    renderToast({ undo: { kind: 'files', ok: false, count: 1, total: 3, reason: 'source_missing' }, error: true });
    expect(screen.getByRole('alert')).toHaveTextContent('Rückgängig: 1 von 3 zurückgelegt, der Rest nicht möglich: Datei fehlt auf der Festplatte');
  });
});

it('RenameToast offers undo only when it can be undone', () => {
  const onUndo = vi.fn();
  const { rerender } = render(<LanguageProvider><RenameToast from="Parts" to="Teile" count={2} onDone={() => {}} /></LanguageProvider>);
  expect(screen.queryByRole('button', { name: 'Rückgängig' })).toBeNull();
  rerender(<LanguageProvider><RenameToast from="Parts" to="Teile" count={2} onDone={() => {}} onUndo={onUndo} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Rückgängig' }));
  expect(onUndo).toHaveBeenCalled();
  expect(screen.getByRole('status')).toHaveTextContent('Strg+Z');
});
