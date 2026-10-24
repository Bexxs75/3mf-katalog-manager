import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import { invoke } from '@tauri-apps/api/core';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ModelDetailPage } from './ModelDetailPage';
import { LanguageProvider } from '../i18n/LanguageContext';
import { makeModelFile, makePrinter } from '../test/factories';
import { de } from '../i18n/de';
import { FILAMENT_USAGE_HELP_URL } from '../lib/filamentUsageHelp';

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

it('asks before deleting and leaves the model alone when cancelled', () => {
  const onDelete = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} onDelete={onDelete} /></LanguageProvider>);
  const trigger = screen.getByRole('button', {name: 'Löschen'});
  fireEvent.click(trigger);
  expect(onDelete).not.toHaveBeenCalled();
  expect(screen.getByText('Eintrag löschen?')).toBeVisible();
  expect(screen.getByRole('button', {name: 'Abbrechen'})).toHaveFocus();
  fireEvent.click(screen.getByRole('button', {name: 'Abbrechen'}));
  expect(onDelete).not.toHaveBeenCalled();
  expect(screen.queryByText('Eintrag löschen?')).toBeNull();
  expect(screen.getByRole('button', {name: 'Löschen'})).toHaveFocus();
});

it('cancels the delete question with Escape without closing the page', () => {
  const onDelete = vi.fn();
  const onClose = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} onDelete={onDelete} onClose={onClose} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
  fireEvent.keyDown(screen.getByRole('button', {name: 'Abbrechen'}), {key: 'Escape'});
  expect(screen.queryByText('Eintrag löschen?')).toBeNull();
  expect(screen.getByRole('button', {name: 'Löschen'})).toHaveFocus();
  expect(onDelete).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
});

it('cancels the delete question with Escape from outside the question without closing the page', () => {
  const onClose = vi.fn();
  const bubbled = vi.fn();
  window.addEventListener('keydown', bubbled);
  try {
    render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} onClose={onClose} /></LanguageProvider>);
    fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
    const rescan = screen.getByRole('button', {name: 'Metadaten neu einlesen'});
    rescan.focus();
    fireEvent.keyDown(rescan, {key: 'Escape'});
    expect(screen.queryByText('Eintrag löschen?')).toBeNull();
    expect(bubbled).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('button', {name: 'Löschen'})).toHaveFocus();
  } finally {
    window.removeEventListener('keydown', bubbled);
  }
});

it('deletes only after the question is confirmed', () => {
  const onDelete = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} onDelete={onDelete} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
  fireEvent.click(screen.getByRole('button', {name: 'Löschen'}));
  expect(onDelete).toHaveBeenCalledTimes(1);
});


it('shortens long tag chips but removes the full tag', () => {
  const tag = 'a'.repeat(95);
  const onRemoveTag = vi.fn();
  render(<LanguageProvider><ModelDetailPage {...props} model={makeModelFile({ tags: [tag] })} hasPrevious={false} hasNext={false} onRemoveTag={onRemoveTag} /></LanguageProvider>);
  const text = screen.getByText('#' + 'a'.repeat(28) + '…');
  expect(text).toHaveClass('whitespace-nowrap');
  expect(text.parentElement).toHaveAttribute('title', '#' + tag);
  expect(text.parentElement).toHaveClass('max-w-full', 'min-w-0');
  const remove = screen.getByRole('button', { name: tag + ' entfernen' });
  expect(remove).toHaveClass('flex-none');
  fireEvent.click(remove);
  expect(onRemoveTag).toHaveBeenCalledWith(tag);
});

it('gives the tag entry, date and note fields a border for the shared accent focus style', () => {
  render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: '+ Eintrag hinzufügen' }));
  const date = document.querySelector<HTMLInputElement>('input[type="date"]');
  const fields = [screen.getByLabelText('Tag hinzufügen'), date, screen.getByPlaceholderText('Notiz (optional)')];
  for (const field of fields) {
    // theme.css colors the border on focus; without one the field would have no indicator at all.
    expect(field).toHaveClass('border');
    expect(field).not.toHaveClass('outline-0');
  }
});

it('shows the companion files of the model on the detail page', async () => {
  vi.mocked(invoke).mockImplementation((command: string) => Promise.resolve(command === 'list_companions'
    ? [{ id: 'c1', fileId: 'model-1', name: 'Anleitung.pdf', path: '/c/Anleitung.pdf', kind: 'pdf', sizeBytes: 10, addedAt: '', deletedAt: null, missing: false }]
    : []));
  try {
    render(<LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} /></LanguageProvider>);
    expect(await screen.findByRole('heading', { name: 'Begleitdateien (1)' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Anleitung.pdf öffnen' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Hinzufügen …' })).toBeEnabled();
  } finally {
    vi.mocked(invoke).mockImplementation(() => Promise.resolve([]));
  }
});

const renderPage = (extra: Partial<React.ComponentProps<typeof ModelDetailPage>> = {}) => render(
  <LanguageProvider><ModelDetailPage {...props} hasPrevious={false} hasNext={false} {...extra} /></LanguageProvider>);
const sliceInfo = { totalWeightG: 5, plates: [{ plateIndex: 1, weightG: 5, filaments: [{ type: 'PLA', color: null, usedG: 5, usedM: 1.6 }] }] };

it('explains a missing filament usage for a loaded 3MF and links to the manual', () => {
  renderPage({ model: makeModelFile({ path: '/c/a.3mf', sliceInfo: null }) });
  const hint = screen.getByRole('status');
  expect(hint).toHaveTextContent(de.filamentUsageMissingTitle);
  expect(hint).toHaveTextContent(de.filamentUsageMissingBody);
  expect(screen.getByRole('link', { name: de.filamentUsageMissingLink })).toHaveAttribute('href', FILAMENT_USAGE_HELP_URL);
});

it('shows the usage and no hint when the 3MF has slicer data', () => {
  renderPage({ model: makeModelFile({ path: '/c/a.3mf', sliceInfo }) });
  expect(screen.queryByText(de.filamentUsageMissingTitle)).toBeNull();
  expect(screen.getByText(de.sliceFilamentHeading)).toBeVisible();
});

it.each(['stl', 'obj', 'step'])('shows no usage hint for a %s file', extension => {
  renderPage({ model: makeModelFile({ path: `/c/a.${extension}`, sliceInfo: null }) });
  expect(screen.queryByText(de.filamentUsageMissingTitle)).toBeNull();
});

it('shows no usage hint while only the list summary is loaded', () => {
  renderPage({ model: makeModelFile({ path: '/c/a.3mf', sliceInfo: null, summaryOnly: true }) });
  expect(screen.queryByText(de.filamentUsageMissingTitle)).toBeNull();
});

it('lists the print bed verdict per printer on the detail page, without printers lacking a build volume', () => {
  renderPage({
    model: makeModelFile({ dimensionsMm: [220, 180, 140] }),
    printers: [makePrinter({ id: 'a', name: 'X1C' }), makePrinter({ id: 'b', name: 'Mini', bedXMm: 180, bedYMm: 180, bedZMm: 180 }),
      makePrinter({ id: 'c', name: 'NoBed', bedXMm: null })],
  });
  const section = screen.getByRole('region', { name: de.plateFitHeading });
  expect(section).toHaveTextContent('Modell 220 × 180 × 140 mm');
  const rows = within(section).getAllByRole('listitem');
  expect(rows).toHaveLength(2);
  expect(rows[0]).toHaveTextContent(`${de.plateFitFits}X1C (256 × 256 × 256)`);
  expect(rows[1]).toHaveTextContent(`${de.plateFitTooLarge}Mini (180 × 180 × 180): 220 mm zu breit`);
  expect(section).not.toHaveTextContent('NoBed');
});

it('shows no print bed section without printers that have a build volume', () => {
  renderPage({ model: makeModelFile({ dimensionsMm: [10, 10, 10] }), printers: [makePrinter({ bedZMm: 0 })] });
  expect(screen.queryByRole('region', { name: de.plateFitHeading })).toBeNull();
});
