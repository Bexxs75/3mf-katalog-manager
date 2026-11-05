import { useRef, useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { BundleDialog, type BundleDialogRequest } from './BundleDialog';
import { de } from '../i18n/de';
import { makeModelFile } from '../test/factories';
import type { Bundle } from '../types';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const models = [
  makeModelFile({ id: '1', name: 'Halter gross.3mf', fileSizeBytes: 2048, contentHash: 'h1' }),
  makeModelFile({ id: '2', name: 'Halter klein.3mf', fileSizeBytes: 1024, contentHash: 'h2' }),
  makeModelFile({ id: '3', name: 'Halter klein (Kopie).3mf', fileSizeBytes: 1024, contentHash: 'h2' }),
  makeModelFile({ id: '4', name: 'Deckel.stl', fileSizeBytes: 512 }),
];
const harbour: Bundle = {
  id: 'b1', name: 'Hafen', kind: 'variants', createdAt: '2026-11-01T00:00:00Z',
  members: [
    { fileId: '1', position: 0, label: 'gross', inTrash: false },
    { fileId: '2', position: 1, label: null, inTrash: false },
    { fileId: '4', position: 2, label: null, inTrash: false },
  ],
};
const other: Bundle = {
  id: 'b2', name: 'Paar', kind: 'parts', createdAt: '2026-11-01T00:00:00Z',
  members: [{ fileId: '3', position: 0, label: null, inTrash: false }, { fileId: '9', position: 1, label: null, inTrash: false }],
};

function Harness({ request, bundles, handlers }: { request: BundleDialogRequest; bundles: Bundle[]; handlers: Partial<Parameters<typeof BundleDialog>[0]> }) {
  const opener = useRef<HTMLButtonElement>(null);
  return <>
    <button ref={opener}>opener</button>
    <BundleDialog request={request} models={models} bundles={bundles} returnFocus={opener} onClose={vi.fn()}
      onCreate={vi.fn().mockResolvedValue(undefined)} onAdd={vi.fn().mockResolvedValue(undefined)} onSaveEdit={vi.fn().mockResolvedValue(undefined)} {...handlers} />
  </>;
}
const open = (request: BundleDialogRequest, bundles: Bundle[] = [], handlers: Partial<Parameters<typeof BundleDialog>[0]> = {}) =>
  render(<LanguageProvider><Harness request={request} bundles={bundles} handlers={handlers} /></LanguageProvider>);
const rowNames = () => [...document.querySelectorAll('[data-row]')].map(row => row.querySelector('.font-semibold')?.textContent);

describe('BundleDialog, create', () => {
  it('starts with the name of the title model, the first kind and the selection in order, and focuses the name', () => {
    open({ mode: 'create', fileIds: ['1', '2', '4'] });
    const dialog = screen.getByRole('dialog', { name: de.bundleDlgCreateTitle });
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    const name = within(dialog).getByLabelText(de.bundleDlgName) as HTMLInputElement;
    expect(name.value).toBe('Halter gross');
    expect(name).toHaveFocus();
    expect(within(dialog).getByRole('radio', { name: new RegExp(de.bundleKindVariants) })).toBeChecked();
    expect(rowNames()).toEqual(['Halter gross.3mf', 'Halter klein.3mf', 'Deckel.stl']);
    expect(within(dialog).getByRole('button', { name: 'Bündel anlegen (3 Modelle)' })).toBeEnabled();
  });

  it('hands name, kind, order and trimmed labels to the save action and closes afterwards', async () => {
    const onCreate = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    open({ mode: 'create', fileIds: ['1', '2', '4'] }, [], { onCreate, onClose });
    fireEvent.change(screen.getByLabelText(de.bundleDlgName), { target: { value: ' Hafenset ' } });
    fireEvent.click(screen.getByRole('radio', { name: /^Teile/ }));
    fireEvent.change(screen.getByLabelText(/Beschriftung für Deckel\.stl/), { target: { value: ' Deckel ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Nach oben: Deckel.stl' }));
    fireEvent.click(screen.getByRole('button', { name: 'Bündel anlegen (3 Modelle)' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(onCreate).toHaveBeenCalledWith('Hafenset', 'parts', [
      { fileId: '1', label: '' }, { fileId: '4', label: 'Deckel' }, { fileId: '2', label: '' },
    ]);
  });

  it('changes the order with the arrow buttons and keeps the focus in place, announcing the new position', () => {
    open({ mode: 'create', fileIds: ['1', '2', '4'] });
    fireEvent.click(screen.getByRole('button', { name: 'Nach unten: Halter gross.3mf' }));
    expect(rowNames()).toEqual(['Halter klein.3mf', 'Halter gross.3mf', 'Deckel.stl']);
    expect(screen.getByRole('button', { name: 'Nach unten: Halter gross.3mf' })).toHaveFocus();
    expect(screen.getByRole('status')).toHaveTextContent('Halter gross.3mf: jetzt Position 2 von 3.');
    // At the end the pressed button is disabled, so the partner takes the focus.
    fireEvent.click(screen.getByRole('button', { name: 'Nach unten: Halter gross.3mf' }));
    expect(screen.getByRole('button', { name: 'Nach oben: Halter gross.3mf' })).toHaveFocus();
  });

  it('changes the order from the keyboard on the grip and says when a file becomes the title model', () => {
    open({ mode: 'create', fileIds: ['1', '2', '4'] });
    const grip = screen.getByRole('button', { name: /Reihenfolge ändern: Deckel\.stl, Position 3 von 3/ });
    grip.focus();
    fireEvent.keyDown(grip, { key: 'ArrowUp' });
    fireEvent.keyDown(screen.getByRole('button', { name: /Reihenfolge ändern: Deckel\.stl, Position 2 von 3/ }), { key: 'ArrowUp' });
    expect(rowNames()).toEqual(['Deckel.stl', 'Halter gross.3mf', 'Halter klein.3mf']);
    expect(screen.getByRole('status')).toHaveTextContent('Deckel.stl: jetzt Position 1 von 3 (Titelmodell).');
    expect(screen.getByRole('button', { name: /Reihenfolge ändern: Deckel\.stl, Position 1 von 3/ })).toHaveFocus();
  });

  it('makes a file the title model by swapping places and moves a row when dropped', () => {
    open({ mode: 'create', fileIds: ['1', '2', '4'] });
    fireEvent.click(within(document.querySelectorAll<HTMLElement>('[data-row]')[2]).getByRole('button', { name: de.bundleDlgMakeTitle }));
    expect(rowNames()).toEqual(['Deckel.stl', 'Halter klein.3mf', 'Halter gross.3mf']);
    const rows = document.querySelectorAll<HTMLElement>('[data-row]');
    fireEvent.mouseDown(within(rows[0]).getByRole('button', { name: /Reihenfolge ändern/ }));
    fireEvent.drop(document.querySelectorAll<HTMLElement>('[data-row]')[2]);
    expect(rowNames()).toEqual(['Halter klein.3mf', 'Halter gross.3mf', 'Deckel.stl']);
  });

  it('takes a model out of the selection but never goes below two', () => {
    open({ mode: 'create', fileIds: ['1', '2', '4'] });
    fireEvent.click(screen.getAllByRole('button', { name: de.bundleDlgTakeOut })[1]);
    expect(rowNames()).toEqual(['Halter gross.3mf', 'Deckel.stl']);
    for (const button of screen.getAllByRole('button', { name: de.bundleDlgTakeOut })) {
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute('title', de.bundleDlgNeedTwo);
    }
    expect(screen.getByRole('button', { name: 'Bündel anlegen (2 Modelle)' })).toBeEnabled();
  });

  it('refuses an empty name with a message that is announced', () => {
    open({ mode: 'create', fileIds: ['1', '2'] });
    fireEvent.change(screen.getByLabelText(de.bundleDlgName), { target: { value: '   ' } });
    expect(screen.getByRole('alert')).toHaveTextContent(de.bundleDlgNameEmpty);
    expect(screen.getByRole('button', { name: /Bündel anlegen/ })).toBeDisabled();
  });

  it('marks an identical file as a duplicate', () => {
    open({ mode: 'create', fileIds: ['2', '3'] });
    const rows = document.querySelectorAll<HTMLElement>('[data-row]');
    expect(within(rows[0]).queryByText(de.bundleDlgDuplicate)).toBeNull();
    expect(within(rows[1]).getByText(de.bundleDlgDuplicate)).toBeInTheDocument();
  });

  it('needs a confirmation before a file switches from another bundle, and says what is left of the old one', () => {
    open({ mode: 'create', fileIds: ['1', '3'] }, [harbour, other]);
    const note = document.querySelector('[data-switch-note]') as HTMLElement;
    expect(note).toHaveTextContent('„Halter gross.3mf“ wechselt von Bündel „Hafen“ zu „Halter gross“.');
    expect(note).toHaveTextContent('Das Bündel „Hafen“ behält 2 Modelle.');
    expect(note).toHaveTextContent('„Halter klein (Kopie).3mf“ wechselt von Bündel „Paar“');
    expect(note).toHaveTextContent('Das Bündel „Paar“ hätte danach weniger als 2 Modelle und wird aufgelöst.');
    const submit = screen.getByRole('button', { name: /Bündel anlegen/ });
    expect(submit).toBeDisabled();
    fireEvent.click(within(note).getByRole('checkbox', { name: de.bundleDlgSwitchConfirm }));
    expect(submit).toBeEnabled();
  });

  it('shows a failure in the dialog and stays open', async () => {
    const onClose = vi.fn();
    open({ mode: 'create', fileIds: ['1', '2'] }, [], { onCreate: vi.fn().mockRejectedValue(new Error('boom')), onClose });
    fireEvent.click(screen.getByRole('button', { name: /Bündel anlegen/ }));
    expect(await screen.findByText(/boom/)).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: /Bündel anlegen/ })).toBeEnabled();
  });

  it('closes on Escape and gives the focus back to the opener', () => {
    function Host() {
      const [shown, setShown] = useState(true);
      const opener = useRef<HTMLButtonElement>(null);
      return <>
        <button ref={opener}>opener</button>
        {shown && <BundleDialog request={{ mode: 'create', fileIds: ['1', '2'] }} models={models} bundles={[]} returnFocus={opener}
          onClose={() => setShown(false)} onCreate={vi.fn()} onAdd={vi.fn()} onSaveEdit={vi.fn()} />}
      </>;
    }
    render(<LanguageProvider><Host /></LanguageProvider>);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('button', { name: 'opener' })).toHaveFocus();
  });

  it('keeps Tab inside the dialog', () => {
    open({ mode: 'create', fileIds: ['1', '2'] });
    const dialog = screen.getByRole('dialog');
    const buttons = within(dialog).getAllByRole('button').filter(button => !(button as HTMLButtonElement).disabled);
    const last = buttons[buttons.length - 1];
    last.focus();
    fireEvent.keyDown(document, { key: 'Tab' });
    expect(dialog.contains(document.activeElement)).toBe(true);
  });
});

describe('BundleDialog, edit', () => {
  it('starts with the values of the bundle and saves nothing but the draft it was given', async () => {
    const onSaveEdit = vi.fn().mockResolvedValue(undefined);
    open({ mode: 'edit', bundleId: 'b1' }, [harbour], { onSaveEdit });
    expect((screen.getByLabelText(de.bundleDlgName) as HTMLInputElement).value).toBe('Hafen');
    expect((screen.getByLabelText(/Beschriftung für Halter gross\.3mf/) as HTMLInputElement).value).toBe('gross');
    expect(rowNames()).toEqual(['Halter gross.3mf', 'Halter klein.3mf', 'Deckel.stl']);
    fireEvent.click(screen.getByRole('button', { name: de.bundleDlgSave }));
    await waitFor(() => expect(onSaveEdit).toHaveBeenCalled());
    expect(onSaveEdit.mock.calls[0][0]).toBe(harbour);
    expect(onSaveEdit.mock.calls[0][1]).toEqual({
      name: 'Hafen', kind: 'variants',
      rows: [{ fileId: '1', label: 'gross', keep: true }, { fileId: '2', label: '', keep: true }, { fileId: '4', label: '', keep: true }],
    });
  });

  it('marks a removal until save, lets it be undone, and offers to dissolve when too few stay', () => {
    open({ mode: 'edit', bundleId: 'b1' }, [harbour]);
    fireEvent.click(screen.getAllByRole('button', { name: de.bundleDlgRemove })[1]);
    expect(screen.getByText(de.bundleDlgWillRemove)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.bundleDlgSave })).toBeEnabled();
    fireEvent.click(screen.getByRole('button', { name: de.bundleDlgKeep }));
    expect(screen.queryByText(de.bundleDlgWillRemove)).toBeNull();
    fireEvent.click(screen.getAllByRole('button', { name: de.bundleDlgRemove })[0]);
    fireEvent.click(screen.getAllByRole('button', { name: de.bundleDlgRemove })[0]);
    expect(screen.getByText(de.bundleDlgTooFew)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.bundleDissolve })).toBeEnabled();
  });

  it('closes by itself when the bundle is gone', () => {
    const onClose = vi.fn();
    open({ mode: 'edit', bundleId: 'missing' }, [harbour], { onClose });
    expect(onClose).toHaveBeenCalled();
  });
});

describe('BundleDialog, add', () => {
  it('appends the chosen models, skips those that already belong and counts against the limit', async () => {
    const onAdd = vi.fn().mockResolvedValue(undefined);
    open({ mode: 'add', fileIds: ['1', '3', '2'], targetId: null }, [harbour, other], { onAdd });
    const dialog = screen.getByRole('dialog', { name: de.bundleDlgAddTitle });
    expect(within(dialog).getByLabelText(de.bundleDlgTarget)).toHaveValue('b1');
    // 1 and 2 are in "Hafen" already; only 3 joins.
    expect(within(dialog).getByRole('checkbox', { name: 'Halter gross.3mf hinzufügen' })).toBeDisabled();
    expect(within(dialog).getAllByText(de.bundleDlgAlreadyIn, { exact: false })).toHaveLength(2);
    expect(dialog).toHaveTextContent('Danach hat „Hafen“ 4 von höchstens 200 Modellen.');
    const confirm = within(dialog).getByRole('checkbox', { name: de.bundleDlgSwitchConfirm });
    const submit = within(dialog).getByRole('button', { name: '1 Modell hinzufügen' });
    expect(submit).toBeDisabled();
    fireEvent.click(confirm);
    fireEvent.click(submit);
    await waitFor(() => expect(onAdd).toHaveBeenCalledWith('b1', ['3']));
  });

  it('lets a model be left out and then has nothing to add', () => {
    open({ mode: 'add', fileIds: ['4'], targetId: 'b2' }, [harbour, other]);
    expect(screen.getByLabelText(de.bundleDlgTarget)).toHaveValue('b2');
    fireEvent.click(screen.getByRole('checkbox', { name: 'Deckel.stl hinzufügen' }));
    expect(screen.getByRole('button', { name: '0 Modelle hinzufügen' })).toBeDisabled();
  });
});
