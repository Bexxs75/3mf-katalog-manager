import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DuplicatesView } from './DuplicatesView';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { makeModelFile } from '../test/factories';
import { de } from '../i18n/de';
import type { ModelFile } from '../types';

const web = vi.hoisted(() => ({ canManage: true }));
vi.mock('../lib/webSession', async importOriginal => ({ ...(await importOriginal<typeof import('../lib/webSession')>()), useCanManageFiles: () => web.canManage }));

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); web.canManage = true; });

const m = (id: string, hash: string | null, importedAt: string, extra: Partial<ModelFile> = {}) =>
  makeModelFile({ id, name: `${id}.3mf`, path: `/catalog/${id}.3mf`, contentHash: hash, importedAt, ...extra });
const models = [
  m('a-old', 'h1', '2026-01-01T10:00:00Z', { tags: ['x', 'y'], printStatus: 'printed' }),
  m('a-new', 'h1', '2026-03-01T10:00:00Z'),
  m('b-old', 'h2', '2026-02-01T10:00:00Z'),
  m('b-mid', 'h2', '2026-02-10T10:00:00Z'),
  m('b-new', 'h2', '2026-02-20T10:00:00Z'),
  m('single', 'h3', '2026-02-01T10:00:00Z'),
  m('nohash', null, '2026-02-01T10:00:00Z'),
];

function show(onRemove?: (ids: string[]) => Promise<string[]>, list = models) {
  return render(<LanguageProvider><DuplicatesView models={list} onRemove={onRemove} /></LanguageProvider>);
}
const group = (n: number) => screen.getByRole('region', { name: `Gruppe ${n}` });
const kept = (n: number) => within(group(n)).getAllByRole('radio').filter(r => (r as HTMLInputElement).checked).map(r => r.getAttribute('aria-label'));

describe('DuplicatesView', () => {
  it('shows only groups of at least two models, none for single or hash-less models, oldest kept by default', () => {
    show(vi.fn());
    expect(screen.getAllByRole('region')).toHaveLength(2);
    expect(screen.queryByText(/single\.3mf/)).toBeNull();
    expect(screen.queryByText(/nohash\.3mf/)).toBeNull();
    expect(within(group(1)).getAllByRole('radio')).toHaveLength(2);
    expect(within(group(2)).getAllByRole('radio')).toHaveLength(3);
    expect(kept(1)).toEqual(['Behalten: a-old.3mf (/catalog/a-old.3mf)']);
    expect(kept(2)).toEqual(['Behalten: b-old.3mf (/catalog/b-old.3mf)']);
    expect(within(group(1)).getByText(de.dvKeepLabel)).toBeVisible();
    expect(within(group(1)).getByText(de.dvDropLabel)).toBeVisible();
    expect(within(group(1)).getByText(de.dvPrinted)).toBeVisible();
    expect(within(group(1)).getByText(/2 Tags/)).toBeVisible();
    expect(within(group(1)).getByText(new RegExp(de.dvNoTags))).toBeVisible();
  });

  it('keeps exactly one entry per group at every step and counts the deselected ones', () => {
    show(vi.fn());
    const summary = () => screen.getByText(/Gruppen, \d+ Einträge abgewählt/);
    expect(summary()).toHaveTextContent('2 Gruppen, 3 Einträge abgewählt');
    expect(summary()).toHaveAttribute('aria-live', 'polite');
    fireEvent.click(within(group(2)).getByRole('radio', { name: /b-new/ }));
    expect(kept(2)).toEqual(['Behalten: b-new.3mf (/catalog/b-new.3mf)']);
    fireEvent.click(screen.getByRole('button', { name: de.dvKeepNewest }));
    expect(kept(1)).toEqual(['Behalten: a-new.3mf (/catalog/a-new.3mf)']);
    expect(kept(2)).toEqual(['Behalten: b-new.3mf (/catalog/b-new.3mf)']);
    fireEvent.click(screen.getByRole('button', { name: de.dvKeepOldest }));
    expect(kept(1)).toEqual(['Behalten: a-old.3mf (/catalog/a-old.3mf)']);
    expect(kept(2)).toEqual(['Behalten: b-old.3mf (/catalog/b-old.3mf)']);
    for (const n of [1, 2]) expect(kept(n)).toHaveLength(1);
    expect(summary()).toHaveTextContent('2 Gruppen, 3 Einträge abgewählt');
  });

  it('asks first, starts on Cancel, and removes nothing when cancelled or closed with Escape', () => {
    const onRemove = vi.fn();
    show(onRemove);
    const apply = screen.getByRole('button', { name: de.dvApply });
    apply.focus();
    fireEvent.click(apply);
    const dialog = screen.getByRole('dialog', { name: de.dvConfirmTitle });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(within(dialog).getByText('3 Modelle kommen in den App-Papierkorb (7 Tage, wiederherstellbar)')).toBeVisible();
    expect(within(dialog).getByText(de.dvConfirmLoss)).toBeVisible();
    expect(within(dialog).getByRole('button', { name: de.cancel })).toHaveFocus();
    fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(apply).toHaveFocus();
    fireEvent.click(apply);
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: de.cancel }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(onRemove).not.toHaveBeenCalled();
  });

  it('removes exactly the deselected ids, says how many went, and puts the focus on the heading', async () => {
    const onRemove = vi.fn().mockResolvedValue([]);
    show(onRemove);
    fireEvent.click(within(group(2)).getByRole('radio', { name: /b-mid/ }));
    fireEvent.click(screen.getByRole('button', { name: de.dvApply }));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: de.dvConfirmRemove }));
    await waitFor(() => expect(onRemove).toHaveBeenCalledTimes(1));
    expect([...onRemove.mock.calls[0][0]].sort()).toEqual(['a-new', 'b-new', 'b-old']);
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(screen.getByRole('status')).toHaveTextContent('3 Modelle in den Papierkorb gelegt');
    expect(screen.getByRole('heading', { name: de.toolDuplicates })).toHaveFocus();
  });

  it('names the models that stayed because the backend skipped them', async () => {
    show(vi.fn().mockResolvedValue(['a-new']));
    fireEvent.click(screen.getByRole('button', { name: de.dvApply }));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: de.dvConfirmRemove }));
    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('2 Modelle in den Papierkorb gelegt'));
    expect(screen.getByRole('status')).toHaveTextContent('1 Modell konnte nicht entfernt werden');
  });

  it('keeps the dialog open and shows the error when removing fails', async () => {
    show(vi.fn().mockRejectedValue({ message: 'boom', expected: false }));
    fireEvent.click(screen.getByRole('button', { name: de.dvApply }));
    fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: de.dvConfirmRemove }));
    const dialog = screen.getByRole('dialog');
    await waitFor(() => expect(within(dialog).getByRole('alert')).toHaveTextContent(de.dvFailed));
    expect(within(dialog).getByRole('button', { name: de.dvConfirmRemove })).toBeEnabled();
  });

  it('offers no removal without a handler or without the right to manage files, but still shows the choice', () => {
    const { unmount } = show(undefined);
    expect(screen.queryByRole('button', { name: de.dvApply })).toBeNull();
    expect(screen.getByText(de.webReadOnly)).toBeVisible();
    expect(screen.getAllByRole('region')).toHaveLength(2);
    unmount();
    web.canManage = false;
    show(vi.fn());
    expect(screen.queryByRole('button', { name: de.dvApply })).toBeNull();
    expect(screen.getAllByRole('radio').length).toBe(5);
  });

  it('says so when there are no duplicates', () => {
    show(vi.fn(), [models[5], models[6]]);
    expect(screen.getByText(de.dvEmpty)).toBeVisible();
    expect(screen.queryByRole('button', { name: de.dvApply })).toBeNull();
  });
});
