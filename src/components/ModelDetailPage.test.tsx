import { invoke } from '@tauri-apps/api/core';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ModelDetailPage } from './ModelDetailPage';
import { LanguageProvider } from '../i18n/LanguageContext';
import { makeModelFile } from '../test/factories';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <button>Viewer control</button> }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => ({}) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve([])), convertFileSrc: (s: string) => s }));

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); vi.restoreAllMocks(); });
const props = {
  model: makeModelFile(), allTags: [], onClose: vi.fn(), onAddTag: vi.fn(), onRemoveTag: vi.fn(),
  onDelete: vi.fn(), onTogglePrintStatus: vi.fn(), onToggleFavorite: vi.fn(), onToggleQueue: vi.fn(),
  onUploadImage: vi.fn(), onSnapshotCaptured: vi.fn(), onSetSourceUrl: vi.fn(), onOpenInSlicer: vi.fn(),
  onRescanMetadata: vi.fn(), onAddToCollection: vi.fn(), collections: [], slicers: [], slicerError: null,
  rescanError: null, rescanSuccess: false, displayPreference: 'thumbnail' as const,
};

it('renders position and labelled buttons and respects both boundaries and a single model', () => {
  const onNavigate = vi.fn();
  const page = (hasPrevious: boolean, hasNext: boolean) => <LanguageProvider><ModelDetailPage {...props}
    onNavigate={onNavigate} hasPrevious={hasPrevious} hasNext={hasNext} position={{index: 3, total: 148}} /></LanguageProvider>;
  const { rerender } = render(page(true, true));
  expect(screen.getByText('3 von 148')).toBeVisible();
  fireEvent.click(screen.getByLabelText('Vorheriges Modell'));
  fireEvent.click(screen.getByLabelText('Nächstes Modell'));
  expect(onNavigate.mock.calls).toEqual([['previous'], ['next']]);
  rerender(page(false, true));
  expect(screen.getByLabelText('Vorheriges Modell')).toBeDisabled();
  rerender(page(true, false));
  expect(screen.getByLabelText('Nächstes Modell')).toBeDisabled();
  rerender(page(false, false));
  expect(screen.getByLabelText('Vorheriges Modell')).toBeDisabled();
  expect(screen.getByLabelText('Nächstes Modell')).toBeDisabled();
});

it('blocks draft navigation by keyboard and requires confirmation by button without losing a rejected draft', () => {
  const onNavigate = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} onNavigate={onNavigate} hasPrevious hasNext /></LanguageProvider>);
  const input = screen.getByRole('combobox');
  fireEvent.change(input, {target: {value: 'pending tag'}});
  fireEvent.keyDown(window, {key: 'ArrowRight'});
  expect(onNavigate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText('Nächstes Modell'));
  expect(screen.getByRole('button', {name: 'Weiter bearbeiten'})).toHaveFocus();
  fireEvent.click(screen.getByRole('button', {name: 'Weiter bearbeiten'}));
  expect(onNavigate).not.toHaveBeenCalled();
  expect(input).toHaveValue('pending tag');
  fireEvent.click(screen.getByLabelText('Nächstes Modell'));
  fireEvent.click(screen.getByRole('button', {name: 'Verwerfen und wechseln'}));
  expect(onNavigate).toHaveBeenCalledWith('next');
});

it('gives viewer controls priority and keeps close behavior', () => {
  const onNavigate = vi.fn();
  const onClose = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} onClose={onClose} onNavigate={onNavigate} hasPrevious hasNext /></LanguageProvider>);
  fireEvent.keyDown(screen.getByText('Viewer control'), {key: 'ArrowRight'});
  expect(onNavigate).not.toHaveBeenCalled();
  fireEvent.keyDown(window, {key: 'ArrowRight'});
  expect(onNavigate).toHaveBeenCalledWith('next');
  fireEvent.click(screen.getByTitle('Zurück zum Katalog'));
  expect(onClose).toHaveBeenCalledOnce();
});

it('protects source editing and an open print-log form even after focus leaves the input', () => {
  const onNavigate = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} onNavigate={onNavigate} hasPrevious hasNext /></LanguageProvider>);
  fireEvent.click(screen.getByText(/https:\/\//));
  const source = screen.getByPlaceholderText('https://…');
  fireEvent.change(source, {target: {value: 'https://draft.example/model'}});
  fireEvent.keyDown(window, {key: 'ArrowRight'});
  expect(onNavigate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText('Nächstes Modell'));
  expect(screen.getByRole('dialog')).toBeVisible();
  expect(props.onSetSourceUrl).not.toHaveBeenCalled();
  fireEvent.keyDown(document, {key: 'Escape'});
  expect(screen.getByLabelText('Nächstes Modell')).toHaveFocus();
  expect(source).toHaveValue('https://draft.example/model');
  expect(onNavigate).not.toHaveBeenCalled();
  fireEvent.keyDown(screen.getByPlaceholderText('https://…'), {key: 'Escape'});
  fireEvent.click(screen.getByRole('button', { name: '+ Eintrag hinzufügen' }));
  fireEvent.keyDown(window, {key: 'ArrowRight'});
  expect(onNavigate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText('Nächstes Modell'));
  expect(screen.getByRole('dialog')).toBeVisible();
  fireEvent.click(screen.getByRole('button', {name: 'Weiter bearbeiten'}));
  expect(onNavigate).not.toHaveBeenCalled();
});


it('shows and dismisses an upload failure beside the upload button', async () => {
  const onUploadImage = vi.fn().mockRejectedValue({ message: 'imageUploadUnsupported', expected: true });
  render(<LanguageProvider><ModelDetailPage {...props} onUploadImage={onUploadImage} hasPrevious={false} hasNext={false} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Bild hochladen' }));
  expect(await screen.findByText('Nur PNG-, JPG- oder WebP-Bilder werden unterstützt.')).toBeVisible();
  expect(screen.queryByText('Problem melden')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Hinweis schließen' }));
  expect(screen.queryByRole('alert')).toBeNull();
});

it('reveals the selected model from the lower action bar', () => {
  render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', {name: 'Im Dateimanager anzeigen'}));
  expect(invoke).toHaveBeenCalledWith('reveal_in_file_manager', {fileId: props.model.id});
});

it('allows arrows after clicking either view toggle, retaining focus and overlay guards', () => {
  const onNavigate = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} model={makeModelFile({customImage: 'data:image/png;base64,AA=='})}
    onNavigate={onNavigate} hasPrevious hasNext /></LanguageProvider>);
  for (const name of ['Bild', '3D-Ansicht']) {
    const button = screen.getByRole('button', {name});
    button.focus();
    fireEvent.click(button);
    fireEvent.keyDown(button, {key: 'ArrowRight'});
    expect(onNavigate).toHaveBeenCalledWith('next');
    expect(button).toHaveFocus();
    onNavigate.mockClear();
    for (const markup of ['<div role="menu"></div>', '<div role="dialog"></div>']) {
      document.body.insertAdjacentHTML('beforeend', markup);
      const overlay = document.body.lastElementChild!;
      fireEvent.keyDown(button, {key: 'ArrowRight'});
      expect(onNavigate).not.toHaveBeenCalled();
      overlay.remove();
    }
  }
});
