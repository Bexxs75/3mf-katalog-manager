import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { CatalogCleanupDialog } from './CatalogCleanupDialog';
import { makeModelFile } from '../test/factories';

describe('CatalogCleanupDialog duplicate paths', () => {
  afterEach(() => localStorage.removeItem('3mf-katalog-language'));

  it.each(['de', 'en'])('shows the full path of both the kept and selected duplicate (%s)', (language) => {
    localStorage.setItem('3mf-katalog-language', language);
    const paths = ['/catalog/original/model.3mf', '/documents/important/model.3mf'];
    render(<LanguageProvider><CatalogCleanupDialog
      issues={{ orphaned: [], duplicateGroups: [paths.map((path, index) => makeModelFile({ id: String(index), name: 'model.3mf', path }))] }}
      onClose={vi.fn()} onDelete={vi.fn()}
    /></LanguageProvider>);
    for (const [index, path] of paths.entries()) {
      const text = screen.getByText(path);
      expect(text).toBeVisible();
      expect(text).not.toHaveClass('truncate');
      const checkbox = within(text.closest('label')!).getByRole('checkbox');
      if (index === 0) expect(checkbox).toBeDisabled();
      else expect(checkbox).toBeChecked();
    }
  });
});
