import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { ContextMenu } from './ContextMenu';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

function renderMenu(onRename: (name: string) => Promise<void>) {
  return render(
    <LanguageProvider>
      <ContextMenu
        x={0}
        y={0}
        onClose={() => {}}
        onOpenInSlicer={() => {}}
        onDelete={() => {}}
        inQueue={false}
        onToggleQueue={() => {}}
        printed={false}
        onTogglePrintStatus={() => {}}
        currentName="Wuerfel.stl"
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
