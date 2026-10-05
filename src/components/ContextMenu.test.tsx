import { invoke } from '@tauri-apps/api/core';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ContextMenu } from './ContextMenu';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

function renderMenu(onRename: (name: string) => Promise<void>, onClose = () => {}) {
  return render(
    <LanguageProvider>
      <ContextMenu
        x={0}
        y={0}
        onClose={onClose}
        onOpenInSlicer={() => {}}
        onDelete={() => {}}
        inQueue={false}
        onToggleQueue={() => {}}
        printed={false}
        onTogglePrintStatus={() => {}}
        fileId="42" currentName="Wuerfel.stl"
        onRename={onRename}
      />
    </LanguageProvider>,
  );
}

describe('ContextMenu rename', () => {
  it('shows the message of a backend error object instead of [object Object]', async () => {
    const onRename = vi.fn(() =>
      Promise.reject({ message: 'Dateiname darf keine Pfad-Trennzeichen enthalten', expected: true }),
    );
    renderMenu(onRename);
    fireEvent.click(screen.getByText('Umbenennen'));
    const input = screen.getByDisplayValue('Wuerfel');
    fireEvent.change(input, { target: { value: 'ab/cd' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(await screen.findByText(/Dateiname darf keine Pfad-Trennzeichen enthalten/)).toBeTruthy();
    expect(screen.queryByText(/object Object/)).toBeNull();
  });
});

it('confirms catalog removal separately from deletion and focuses Cancel', async () => {
  const onRemove = vi.fn().mockResolvedValue(undefined);
  render(<LanguageProvider><ContextMenu x={0} y={0} onClose={() => {}} onOpenInSlicer={() => {}}
    onDelete={() => {}} onRemove={onRemove} inQueue={false} onToggleQueue={() => {}}
    printed={false} onTogglePrintStatus={() => {}} fileId="42" currentName="Cube.stl" onRename={async () => {}} /></LanguageProvider>);
  fireEvent.click(screen.getByText('Aus dem Katalog entfernen'));
  expect(screen.getByText('„Cube.stl“ aus dem Katalog entfernen?')).toBeTruthy();
  expect(screen.getByText(/Die Datei bleibt unverändert auf der Festplatte/)).toBeTruthy();
  expect(document.activeElement).toBe(screen.getByText('Abbrechen'));
  fireEvent.click(screen.getByText('Entfernen'));
  expect(onRemove).toHaveBeenCalledOnce();
});

function renderKeyboardMenu() {
  const onDelete = vi.fn();
  const onRemove = vi.fn().mockResolvedValue(undefined);
  const onRename = vi.fn().mockResolvedValue(undefined);
  const onClose = vi.fn();
  const result = render(<LanguageProvider><ContextMenu x={0} y={0} onClose={onClose}
    onOpenInSlicer={vi.fn()} onDelete={onDelete} onRemove={onRemove}
    inQueue={false} onToggleQueue={vi.fn()} printed={false} onTogglePrintStatus={vi.fn()}
    fileId="42" currentName="Cube.stl" onRename={onRename} /></LanguageProvider>);
  return { ...result, onDelete, onRemove, onRename, onClose };
}

describe('ContextMenu keyboard cancellation', () => {
  it('focuses Cancel when entering the delete confirmation', () => {
    renderKeyboardMenu();
    fireEvent.click(screen.getByRole('button', { name: 'Löschen' }));
    expect(screen.getByRole('button', { name: 'Abbrechen' })).toHaveFocus();
  });

  it.each(['Löschen', 'Aus dem Katalog entfernen', 'Umbenennen'])('cancels %s with Escape and keeps focus in the menu', (label) => {
    const { container, onDelete, onRemove, onRename } = renderKeyboardMenu();
    fireEvent.click(screen.getByRole('button', { name: label }));
    const cancel = screen.getByRole('button', { name: 'Abbrechen' });
    cancel.focus();
    fireEvent.keyDown(cancel, { key: 'Escape' });
    expect(screen.queryByRole('button', { name: 'Abbrechen' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Umbenennen' })).toBeInTheDocument();
    expect(container.contains(document.activeElement)).toBe(true);
    expect(onDelete).not.toHaveBeenCalled();
    expect(onRemove).not.toHaveBeenCalled();
    expect(onRename).not.toHaveBeenCalled();
  });
});

it('disables rename, removal and trash while import is active', () => {
  render(<LanguageProvider><ImportLockContext.Provider value={true}><ContextMenu x={0} y={0} onClose={vi.fn()} onDelete={vi.fn()} onRemove={vi.fn()} onOpenInSlicer={vi.fn()} inQueue={false} onToggleQueue={vi.fn()} printed={false} onTogglePrintStatus={vi.fn()} fileId="42" currentName="a.stl" onRename={vi.fn()} /></ImportLockContext.Provider></LanguageProvider>);
  for (const name of [/Umbenennen/, /Katalog entfernen/, /^Löschen$/]) {
    const button = screen.getByRole('button',{name}); expect(button).toBeDisabled(); expect(button).toHaveAttribute('title','Während eines Imports gesperrt');
  }
});

it('reveals the card model and closes only after a successful launch', async () => {
  vi.mocked(invoke).mockResolvedValue(undefined);
  const onClose = vi.fn();
  renderMenu(async () => {}, onClose);
  fireEvent.click(screen.getByRole('button', {name: 'Im Dateimanager anzeigen'}));
  expect(invoke).toHaveBeenCalledWith('reveal_in_file_manager', {fileId: '42'});
  await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
});

it('keeps a failed reveal visible in the context menu', async () => {
  vi.mocked(invoke).mockRejectedValue({message: 'Start fehlgeschlagen', expected: true});
  const onClose = vi.fn();
  renderMenu(async () => {}, onClose);
  fireEvent.click(screen.getByRole('button', {name: 'Im Dateimanager anzeigen'}));
  expect(await screen.findByRole('alert')).toHaveTextContent('Start fehlgeschlagen');
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.queryByRole('button', {name: 'Problem melden'})).not.toBeInTheDocument();
});
