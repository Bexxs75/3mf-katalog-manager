import { useState } from 'react';
import { expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PrintFailureReasonField } from './PrintFailureReasonField';
import { chooseSelect } from '../test/select';

it.each(['mouse', 'keyboard'] as const)('keeps the note for Other and clears it for a specific reason or none (%s)', mode => {
  localStorage.setItem('3mf-katalog-language', 'de');
  const changed = vi.fn();
  function Host() {
    const [reason, setReason] = useState<string | null>(null);
    return <PrintFailureReasonField reason={reason} note="Details" onChange={(next, note) => {
      changed(next, note); setReason(next);
    }} />;
  }
  render(<LanguageProvider><Host /></LanguageProvider>);
  const select = screen.getByRole('combobox', { name: 'Grund (optional)' });
  chooseSelect(select, 'other', mode);
  expect(changed).toHaveBeenLastCalledWith('other', 'Details');
  expect(screen.getByRole('textbox')).toHaveValue('Details');
  chooseSelect(select, 'clog', mode);
  expect(changed).toHaveBeenLastCalledWith('clog', '');
  expect(screen.queryByRole('textbox')).toBeNull();
  chooseSelect(select, '', mode);
  expect(changed).toHaveBeenLastCalledWith(null, '');
});
