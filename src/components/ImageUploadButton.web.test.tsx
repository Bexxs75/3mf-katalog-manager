import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { DetailPanel } from './DetailPanel';
import { ModelDetailPage } from './ModelDetailPage';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { UiDensityProvider } from '../hooks/UiDensityContext';
import { makeModelFile } from '../test/factories';
import { getSession } from '../lib/webSession';
import { de } from '../i18n/de';

vi.mock('./ModelPreview', () => ({ ModelPreview: () => <div>Preview</div> }));
vi.mock('../hooks/usePrintLog', () => ({ usePrintLog: () => ({ entries: [], error: null }) }));
vi.mock('../hooks/useFilamentCheck', () => ({ useFilamentCheck: () => ({}) }));

const noop = () => {};
let writable = true;

beforeEach(async () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  localStorage.setItem('3mf-katalog-density', 'compact');
  writable = true;
  vi.stubGlobal('fetch', vi.fn(async (url: string) => {
    if (!url.startsWith('/api/invoke/')) return Response.json({ authenticated: true, writable, csrfToken: 'token', mustChangePassword: false });
    return Response.json([]);
  }));
  await getSession();
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

const panel = (onUploadImage: (file?: File) => void) => (
  <LanguageProvider><UiDensityProvider><DetailPanel model={makeModelFile()} allTags={[]}
    onAddTag={noop} onRemoveTag={noop} onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop}
    onToggleQueue={noop} onUploadImage={onUploadImage} onSnapshotCaptured={noop} onSetSourceUrl={noop}
    onOpenInSlicer={noop} slicerError={null} /></UiDensityProvider></LanguageProvider>
);
const page = (onRescanMetadata = vi.fn(), onUploadImage = vi.fn()) => (
  <LanguageProvider><ModelDetailPage model={makeModelFile()} allTags={[]} onClose={noop} onAddTag={noop} onRemoveTag={noop}
    onDelete={noop} onTogglePrintStatus={noop} onToggleFavorite={noop} onToggleQueue={noop} onUploadImage={onUploadImage}
    onSnapshotCaptured={noop} onSetSourceUrl={noop} onOpenInSlicer={noop} onRescanMetadata={onRescanMetadata}
    onAddToCollection={noop} collections={[]} slicers={[]} slicerError={null} rescanError={null} rescanSuccess={false}
    displayPreference="thumbnail" hasPrevious={false} hasNext={false} /></LanguageProvider>
);

it('hands the file chosen in the browser to the upload and allows choosing it again', async () => {
  const onUploadImage = vi.fn();
  render(panel(onUploadImage));
  const input = await screen.findByTestId('model-image-input') as HTMLInputElement;
  expect(input.accept).toContain('image/png');
  expect(input.accept).toContain('.webp');
  const file = new File([new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10])], 'cover.png', { type: 'image/png' });
  fireEvent.change(input, { target: { files: [file] } });
  expect(onUploadImage).toHaveBeenCalledWith(file);
  expect(input.value).toBe('');
  // The button itself only opens the chooser; it uploads nothing without a file.
  const open = vi.spyOn(input, 'click').mockImplementation(noop);
  fireEvent.click(screen.getByRole('button', { name: de.uploadModelImageLabel }));
  expect(open).toHaveBeenCalled();
  expect(onUploadImage).toHaveBeenCalledTimes(1);
});

it('disables the image button without write mode and sends nothing even if a file arrives', async () => {
  writable = false; await getSession();
  const onUploadImage = vi.fn();
  render(panel(onUploadImage));
  const button = await screen.findByRole('button', { name: de.uploadModelImageLabel });
  expect(button).toBeDisabled();
  expect(button).toHaveAttribute('title', de.webReadOnly);
  fireEvent.change(screen.getByTestId('model-image-input'), { target: { files: [new File(['x'], 'a.png')] } });
  expect(onUploadImage).not.toHaveBeenCalled();
});

it('shows the same image button on the detail page and reports problems through the shared hook', async () => {
  const onUploadImage = vi.fn(() => Promise.reject({ message: 'imageUploadTooLarge', expected: true }));
  render(page(vi.fn(), onUploadImage));
  fireEvent.change(await screen.findByTestId('model-image-input'), { target: { files: [new File(['x'], 'a.png')] } });
  expect(await screen.findByText(de.imageUploadTooLarge)).toBeVisible();
});

it('enables the rescan button only with write mode', async () => {
  const onRescanMetadata = vi.fn();
  const { unmount } = render(page(onRescanMetadata));
  const button = await screen.findByRole('button', { name: de.rescanMetadataButton });
  expect(button).toBeEnabled();
  fireEvent.click(button);
  expect(onRescanMetadata).toHaveBeenCalledTimes(1);
  unmount();
  writable = false; await getSession();
  render(page(onRescanMetadata));
  const locked = await screen.findByRole('button', { name: de.rescanMetadataButton });
  await waitFor(() => expect(locked).toBeDisabled());
  fireEvent.click(locked);
  expect(onRescanMetadata).toHaveBeenCalledTimes(1);
});
