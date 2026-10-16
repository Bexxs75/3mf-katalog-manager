import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { invoke } from '@tauri-apps/api/core';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ModelDetailPage } from './ModelDetailPage';
import { LanguageProvider } from '../i18n/LanguageContext';
import { makeModelFile } from '../test/factories';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <>
  <button>Viewer control</button>
  <div role="toolbar" aria-label="Viewer tools"><button>Local viewer control</button></div>
</> }));
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

it('navigates from plain viewer buttons, respects widget priority and keeps close behavior', () => {
  const onNavigate = vi.fn();
  const onClose = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} onClose={onClose} onNavigate={onNavigate} hasPrevious hasNext /></LanguageProvider>);
  const button = screen.getByText('Viewer control');
  button.focus();
  fireEvent.keyDown(button, {key: 'ArrowRight'});
  fireEvent.keyDown(button, {key: 'ArrowLeft'});
  expect(onNavigate.mock.calls).toEqual([['next'], ['previous']]);
  expect(button).toHaveFocus();
  onNavigate.mockClear();
  const widgetButton = screen.getByText('Local viewer control');
  widgetButton.focus();
  fireEvent.keyDown(widgetButton, {key: 'ArrowRight'});
  fireEvent.keyDown(widgetButton, {key: 'ArrowLeft'});
  fireEvent.keyDown(window, {key: 'ArrowRight'});
  expect(onNavigate).not.toHaveBeenCalled();
  expect(widgetButton).toHaveFocus();
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
  render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} slicers={[{ id: 's1', name: 'Slicer', path: '/slicer' }]} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', {name: 'Im Dateimanager anzeigen'}));
  expect(invoke).toHaveBeenCalledWith('reveal_in_file_manager', {fileId: props.model.id});
});

it('allows arrows after clicking either view toggle, retaining focus and overlay guards', () => {
  const onNavigate = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} model={makeModelFile({customImage: 'data:image/png;base64,AA=='})}
    onNavigate={onNavigate} hasPrevious hasNext /></LanguageProvider>);
  for (const name of ['Bild', '3D-Ansicht']) {
    const button = screen.getByRole('button', {name});
    const viewer = button.closest<HTMLElement>('[data-detail-viewer]')!;
    viewer.focus();
    expect(viewer).toHaveFocus();
    fireEvent.click(button);
    expect(button).toHaveFocus();
    fireEvent.keyDown(window, {key: 'ArrowRight'});
    fireEvent.keyDown(document.activeElement!, {key: 'ArrowLeft'});
    expect(onNavigate.mock.calls).toEqual([['next'], ['previous']]);
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


it('hides host actions on the container detail page even with a configured slicer', () => {
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: true }}>
    <ModelDetailPage {...props} hasPrevious={false} hasNext={false} slicers={[{ id: 's1', name: 'Slicer', path: '/slicer' }]} />
  </RuntimeEnvironmentProvider></LanguageProvider>);
  expect(screen.queryByRole('button', { name: /In Slicer öffnen/ })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Im Browser öffnen' })).toBeVisible();
  expect(screen.queryByRole('button', { name: /Im Dateimanager anzeigen/ })).not.toBeInTheDocument();
});

it.each([false, true])('renders the source URL for container=%s', container => {
  const sourceUrl = 'https://example.com/model';
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container }}>
    <ModelDetailPage {...props} model={makeModelFile({ sourceUrl })} hasPrevious={false} hasNext={false} />
  </RuntimeEnvironmentProvider></LanguageProvider>);
  if (container) {
    expect(screen.queryByRole('link', { name: sourceUrl })).not.toBeInTheDocument();
    expect(screen.getByText(sourceUrl)).toBeVisible();
    expect(screen.getByRole('button', { name: `Link kopieren: ${sourceUrl}` })).toBeVisible();
  } else expect(screen.getByRole('link', { name: sourceUrl })).toHaveAttribute('href', sourceUrl);
});

it('keeps the desktop slicer action', () => {
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{ container: false }}>
    <ModelDetailPage {...props} hasPrevious={false} hasNext={false} slicers={[{ id: 's1', name: 'Slicer', path: '/slicer' }]} />
  </RuntimeEnvironmentProvider></LanguageProvider>);
  expect(screen.queryByRole('button', { name: 'Im Browser öffnen' })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: /In Slicer öffnen/ }));
  expect(props.onOpenInSlicer).toHaveBeenCalled();
});

it('edits the license on the detail page', async () => {
  const onSetLicense = vi.fn().mockResolvedValue(undefined);
  render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} onSetLicense={onSetLicense} /></LanguageProvider>);
  fireEvent.change(screen.getByRole('combobox', { name: 'Lizenz' }), { target: { value: 'own' } });
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  expect(onSetLicense).toHaveBeenCalledWith([props.model.id], 'own');
});

it.each(['cc0', '', '__other'])('protects a changed license (%s) until discarded', next => {
  const onNavigate = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} model={makeModelFile({license: 'cc-by'})}
    onSetLicense={vi.fn()} onNavigate={onNavigate} hasPrevious={false} hasNext /></LanguageProvider>);
  fireEvent.change(screen.getByRole('combobox', {name: 'Lizenz'}), {target: {value: next}});
  if (next === '__other') fireEvent.change(screen.getByRole('textbox', {name: 'Lizenztext'}), {target: {value: 'My terms'}});
  fireEvent.keyDown(window, {key: 'ArrowRight'});
  expect(onNavigate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText('Nächstes Modell'));
  expect(screen.getByRole('dialog')).toBeVisible();
  fireEvent.click(screen.getByRole('button', {name: 'Verwerfen und wechseln'}));
  expect(onNavigate).toHaveBeenCalledExactlyOnceWith('next');
});

it('does not block unchanged custom text or a saved license', async () => {
  const onNavigate = vi.fn();
  const onSetLicense = vi.fn().mockResolvedValue(undefined);
  render(<LanguageProvider><ModelDetailPage {...props} model={makeModelFile({license: 'My terms'})}
    onSetLicense={onSetLicense} onNavigate={onNavigate} hasPrevious={false} hasNext /></LanguageProvider>);
  fireEvent.click(screen.getByLabelText('Nächstes Modell'));
  expect(onNavigate).toHaveBeenCalledTimes(1);
  fireEvent.change(screen.getByRole('combobox', {name: 'Lizenz'}), {target: {value: 'cc0'}});
  await act(async () => { fireEvent.click(screen.getByRole('button', {name: 'Lizenz speichern'})); });
  fireEvent.click(screen.getByLabelText('Nächstes Modell'));
  expect(onNavigate).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole('dialog')).toBeNull();
});

it('offers original-file download only in the web build', () => {
  const page = <LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} /></LanguageProvider>;
  const { rerender } = render(page);
  expect(screen.queryByRole('button', { name: 'Herunterladen' })).toBeNull();
  vi.stubEnv('VITE_TARGET', 'web');
  rerender(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} /></LanguageProvider>);
  expect(screen.getByRole('button', { name: 'Herunterladen' })).toBeVisible();
  vi.unstubAllEnvs();
});
