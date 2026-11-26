import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { useState } from 'react';
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

describe('CatalogCleanupDialog keyboard and focus', () => {
  afterEach(() => localStorage.removeItem('3mf-katalog-language'));
  const issues = { orphaned: [makeModelFile({ id: '1', name: 'gone.3mf', path: '/r/gone.3mf' })], duplicateGroups: [] };

  it('closes through a real button that Tab can reach', () => {
    const onClose = vi.fn();
    render(<LanguageProvider><CatalogCleanupDialog issues={issues} onClose={onClose} onDelete={vi.fn()} /></LanguageProvider>);
    const close = screen.getByRole('button', { name: de.diskSyncClose });
    expect(close.tagName).toBe('BUTTON');
    fireEvent.click(close);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('gives the focus back to the button that started the scan, even though that button lost it meanwhile', () => {
    const opener = document.createElement('button');
    document.body.append(opener);
    const ref = { current: opener };
    const { unmount } = render(<LanguageProvider><CatalogCleanupDialog issues={issues} onClose={vi.fn()} onDelete={vi.fn()} returnFocus={ref} /></LanguageProvider>);
    expect(opener).not.toHaveFocus();
    unmount();
    expect(opener).toHaveFocus();
    opener.remove();
  });
});

describe('CatalogCleanupDialog stays open for what is left', () => {
  beforeEach(() => vi.stubEnv('VITE_TARGET', 'web'));
  afterEach(() => { vi.unstubAllEnvs(); localStorage.removeItem('3mf-katalog-language'); });
  const gone = (id: string, path: string) => makeModelFile({ id, name: `m${id}.3mf`, path });
  const folders = [{ relativePath: 'Alt', root: 0, models: ['1', '2'] }, { relativePath: 'Neu', root: 0, models: ['3', '4'] }];
  const issues = () => ({ orphaned: [gone('1', '/r/Alt/a.3mf'), gone('2', '/r/Alt/b.3mf'), gone('3', '/r/Neu/c.3mf'), gone('4', '/r/Neu/d.3mf')], duplicateGroups: [], missingFolders: folders });
  /** Like the app: removing hands the ids to the hook, which hands back fewer issues and a notice. */
  function Harness({ start }: { start: ReturnType<typeof issues> }) {
    const [current, setCurrent] = useState(start);
    const [notice, setNotice] = useState<{ count: number } | null>(null);
    return <LanguageProvider><CatalogCleanupDialog issues={current} notice={notice} onClose={vi.fn()}
      onDelete={(ids) => { setCurrent({ ...current, orphaned: current.orphaned.filter(m => !ids.includes(m.id)) }); setNotice({ count: ids.length }); }} /></LanguageProvider>;
  }

  it('keeps the other group after a removal, says what happened and keeps the focus inside', () => {
    render(<Harness start={issues()} />);
    const removeFirst = screen.getAllByRole('button', { name: de.cleanupFolderRemove.other.replace('{count}', '2') })[0];
    removeFirst.focus();
    fireEvent.click(removeFirst);
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.getByRole('status').textContent).toBe(de.cleanupRemovedNotice.other.replace('{count}', '2'));
    expect(screen.getAllByRole('region')).toHaveLength(1);
    expect(screen.getByRole('dialog').contains(document.activeElement)).toBe(true);
    expect(screen.queryByText(de.cleanupNothingLeft)).toBeNull();
  });

  it('shows the empty state with a close button once nothing removable is left', () => {
    render(<Harness start={issues()} />);
    for (let i = 0; i < 2; i++) fireEvent.click(screen.getAllByRole('button', { name: de.cleanupFolderRemove.other.replace('{count}', '2') })[0]);
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.getByText(de.cleanupNothingLeft)).toBeTruthy();
    expect(screen.getAllByRole('button', { name: de.diskSyncClose }).length).toBeGreaterThan(0);
    expect(screen.getByRole('dialog').contains(document.activeElement)).toBe(true);
  });

  it('names the root of an unclear entry when there are two roots, and not with one or with an old server', () => {
    const unclear = [{ path: 'Sub/Einzelteil.3mf', root: 1, reason: 'link' }];
    const view = (roots?: string[]) => {
      const { unmount } = render(<LanguageProvider><CatalogCleanupDialog issues={{ orphaned: [], duplicateGroups: [], unclear, roots }} onClose={vi.fn()} onDelete={vi.fn()} /></LanguageProvider>);
      const text = screen.getByTestId('disk-unclear').textContent ?? '';
      unmount();
      return text;
    };
    expect(view(['alpha', 'beta'])).toContain('beta: Sub/Einzelteil.3mf');
    expect(view(['alpha'])).toContain('Sub/Einzelteil.3mf');
    expect(view(['alpha'])).not.toContain('alpha:');
    expect(view(undefined)).not.toMatch(/\w: Sub/);
  });
});
