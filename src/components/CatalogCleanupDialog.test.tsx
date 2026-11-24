import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { de } from '../i18n/de';
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

describe('CatalogCleanupDialog on the web', () => {
  beforeEach(() => vi.stubEnv('VITE_TARGET', 'web'));
  afterEach(() => { vi.unstubAllEnvs(); localStorage.removeItem('3mf-katalog-language'); });
  const gone = (id: string, path: string) => makeModelFile({ id, name: `m${id}.3mf`, path });
  const show = (issues: Parameters<typeof CatalogCleanupDialog>[0]['issues'], onDelete = vi.fn()) => {
    render(<LanguageProvider><CatalogCleanupDialog issues={issues} onClose={vi.fn()} onDelete={onDelete} /></LanguageProvider>);
    return onDelete;
  };

  it('sums up the models of a missing folder, not preselected, and removes them with one click', () => {
    const onDelete = show({ orphaned: [gone('1', '/r/Alt/a.3mf'), gone('2', '/r/Alt/b.3mf'), gone('3', '/r/Other/c.3mf')], duplicateGroups: [] });
    expect(screen.getByRole('region', { name: de.cleanupFolderMissing.other.replace('{count}', '2') })).toBeTruthy();
    // the lone model stays a plain, preselected row; the folder's models are not in any checkbox
    const boxes = screen.getAllByRole('checkbox') as HTMLInputElement[];
    expect(boxes).toHaveLength(1);
    expect(boxes[0].checked).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: de.cleanupFolderRemove.other.replace('{count}', '2') }));
    expect(onDelete).toHaveBeenCalledWith(['1', '2']);
  });

  it('does not delete a folder through the main button', () => {
    const onDelete = show({ orphaned: [gone('1', '/r/Alt/a.3mf'), gone('2', '/r/Alt/b.3mf')], duplicateGroups: [] });
    expect((screen.getByRole('button', { name: de.cleanupDeleteSelected }) as HTMLButtonElement).disabled).toBe(true);
    expect(onDelete).not.toHaveBeenCalled();
  });

  it('shows unclear models collapsed, without a checkbox, and the truncation notice', () => {
    show({ orphaned: [], duplicateGroups: [], truncated: true, unclear: [{ path: '/nas/x.3mf', reason: 'unreachable' }] });
    const details = screen.getByTestId('disk-unclear') as HTMLDetailsElement;
    expect(details.open).toBe(false);
    expect(within(details).queryAllByRole('checkbox')).toHaveLength(0);
    expect(screen.queryAllByRole('checkbox')).toHaveLength(0);
    expect(screen.getByText(de.cleanupTruncated)).toBeTruthy();
  });

  it('shows a failed removal inside the dialog', () => {
    render(<LanguageProvider><CatalogCleanupDialog issues={{ orphaned: [gone('1', '/r/a.3mf')], duplicateGroups: [] }} onClose={vi.fn()} onDelete={vi.fn()} error={{ message: 'boom', unexpected: false }} /></LanguageProvider>);
    expect(screen.getByRole('alert').textContent).toContain('boom');
  });
});
