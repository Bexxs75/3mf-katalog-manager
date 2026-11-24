import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
vi.mock('../lib/webSession', async importOriginal => ({ ...await importOriginal<typeof import('../lib/webSession')>(), useCanEditMetadata: () => true, canEditMetadata: () => true }));
import { WebRemoveFromCatalogButton } from './WebRemoveFromCatalogButton';
import { LanguageProvider } from '../i18n/LanguageContext';
import { de } from '../i18n/de';
afterEach(() => cleanup());
const show = (count: number, onRemove = vi.fn().mockResolvedValue(undefined)) => {
  render(<LanguageProvider><WebRemoveFromCatalogButton count={count} name="x" onRemove={onRemove} /></LanguageProvider>);
  fireEvent.click(screen.getByRole('button', { name: de.removeCatalog }));
  return onRemove;
};
it('asks for the password from 50 models on and sends it', async () => {
  const onRemove = show(50);
  const confirm = screen.getByRole('button', { name: de.removeEntry }) as HTMLButtonElement;
  expect(confirm.disabled).toBe(true);
  fireEvent.change(screen.getByLabelText(de.webFilePassword), { target: { value: 'secret' } });
  fireEvent.click(confirm);
  await waitFor(() => expect(onRemove).toHaveBeenCalledWith('secret'));
});
it('asks for no password below 50 models and sends none', async () => {
  const onRemove = show(49);
  expect(screen.queryByLabelText(de.webFilePassword)).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: de.removeEntry }));
  await waitFor(() => expect(onRemove).toHaveBeenCalledWith());
});
