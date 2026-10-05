import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { it, expect, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { useImageUpload } from '../hooks/useImageUpload';
import { ImageUploadError } from './ImageUploadError';

function Upload({ id, upload }: { id: string; upload: () => Promise<void> }) {
  const { error, dismiss, uploadImage } = useImageUpload(id, upload);
  return <><button onClick={uploadImage}>Upload</button><ImageUploadError error={error} onDismiss={dismiss} /></>;
}

it('shows translated expected upload errors, dismisses them and clears them when switching model', async () => {
  localStorage.setItem('3mf-katalog-language', 'en');
  const upload = vi.fn().mockRejectedValue({message: 'imageUploadTooLarge', expected: true});
  const page = (id: string) => <LanguageProvider><Upload id={id} upload={upload} /></LanguageProvider>;
  const { rerender } = render(page('1'));
  fireEvent.click(screen.getByText('Upload'));
  expect(await screen.findByText('The image is too large. The maximum size is 5 MB.')).toBeVisible();
  expect(screen.queryByText('Report problem')).toBeNull();
  fireEvent.click(screen.getByRole('button', {name: 'Close notice'}));
  expect(screen.queryByRole('alert')).toBeNull();
  fireEvent.click(screen.getByText('Upload'));
  expect(await screen.findByRole('alert')).toBeVisible();
  rerender(page('2'));
  await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
});

it('ignores a late rejection belonging to a previous model', async () => {
  let reject: (error: unknown) => void = () => {};
  const upload = () => new Promise<void>((_, fail) => { reject = fail; });
  const page = (id: string) => <LanguageProvider><Upload id={id} upload={upload} /></LanguageProvider>;
  const { rerender } = render(page('1'));
  fireEvent.click(screen.getByText('Upload'));
  rerender(page('2'));
  reject({message: 'imageUploadUnsupported', expected: true});
  await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
});
