import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { LicenseEditor } from './LicenseEditor';
import { licensePresets } from '../lib/licenses';
import { de } from '../i18n/de';
import { en } from '../i18n/en';
import { es } from '../i18n/es';
import { fr } from '../i18n/fr';

beforeEach(() => localStorage.clear());
it.each(licensePresets)('saves $code and displays its usage', async preset => {
  const save = vi.fn().mockResolvedValue(undefined);
  render(<LanguageProvider><LicenseEditor value={null} onSave={save} /></LanguageProvider>);
  fireEvent.change(screen.getByRole('combobox', { name: 'Lizenz' }), { target: { value: preset.code } });
  expect(screen.getByText(preset.usage === 'private' ? 'Nur privat' : 'Kommerziell erlaubt')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  await waitFor(() => expect(save).toHaveBeenCalledWith(preset.code));
});
it('preserves free text, limits input and can clear a saved license', async () => {
  const save = vi.fn().mockResolvedValue(undefined);
  const { rerender } = render(<LanguageProvider><LicenseEditor value="cc-by" onSave={save} /></LanguageProvider>);
  fireEvent.change(screen.getByRole('combobox'), { target: { value: '__other' } });
  const input = screen.getByRole('textbox', { name: 'Lizenztext' });
  expect(input).toHaveAttribute('maxlength', '200');
  fireEvent.change(input, { target: { value: '  Contract license  ' } });
  expect(screen.getByText('Nicht angegeben')).toBeVisible();
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  await waitFor(() => expect(save).toHaveBeenCalledWith('  Contract license  '));
  await waitFor(() => expect(screen.getByRole('textbox')).not.toBeDisabled());
  rerender(<LanguageProvider><LicenseEditor value="  Contract license  " onSave={save} /></LanguageProvider>);
  expect(screen.getByRole('textbox')).toHaveValue('  Contract license  ');
  fireEvent.change(screen.getByRole('combobox'), { target: { value: '' } });
  fireEvent.click(screen.getByRole('button', { name: 'Lizenz speichern' }));
  await waitFor(() => expect(save).toHaveBeenLastCalledWith(null));
});
it('shows save failures and permits retry', async () => {
  const save = vi.fn().mockRejectedValue(new Error('Disk full'));
  render(<LanguageProvider><LicenseEditor value={null} onSave={save} /></LanguageProvider>);
  fireEvent.change(screen.getByRole('combobox'), { target: { value: 'own' } });
  fireEvent.click(screen.getByRole('button'));
  expect(await screen.findByRole('alert')).toHaveTextContent('Disk full');
  expect(screen.getByRole('button')).not.toBeDisabled();
});
it('keeps translation keys identical in all languages', () => {
  for (const translation of [en, es, fr]) {
    expect(Object.keys(translation).sort()).toEqual(Object.keys(de).sort());
    for (const preset of licensePresets) expect(translation[preset.label]).toBeTruthy();
  }
});
