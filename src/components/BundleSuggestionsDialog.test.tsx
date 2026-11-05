import { useRef, useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { BundleSuggestionsDialog } from './BundleSuggestionsDialog';
import { suggestBundles, type BundleSuggestion } from '../lib/bundleSuggestions';
import { de } from '../i18n/de';
import { makeModelFile } from '../test/factories';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const models = [
  makeModelFile({ id: '1', name: 'Lampe v1.3mf', fileSizeBytes: 2048, sourcePlatform: 'makerworld', sourceModelId: '77', sourceProfileId: 'p1' }),
  makeModelFile({ id: '2', name: 'Lampe v2.3mf', fileSizeBytes: 1024, sourcePlatform: 'makerworld', sourceModelId: '77', sourceProfileId: 'p2' }),
  makeModelFile({ id: '3', name: 'Schiff Teil 1.3mf', folderId: 'f1' }),
  makeModelFile({ id: '4', name: 'Schiff Teil 2.3mf', folderId: 'f1' }),
];
const folders = [{ id: 'f1', name: 'Figuren', path: '/c/Figuren', parentId: null, count: 2 }];
const all = suggestBundles(models, []);
const byReason = (reason: string) => all.find(suggestion => suggestion.reason === reason)!;

function Harness({ open = all, dismissed = [], handlers = {} }: { open?: BundleSuggestion[]; dismissed?: BundleSuggestion[]; handlers?: Partial<Parameters<typeof BundleSuggestionsDialog>[0]> }) {
  const opener = useRef<HTMLButtonElement>(null);
  const [shown, setShown] = useState(true);
  return <>
    <button ref={opener}>opener</button>
    {shown && <BundleSuggestionsDialog open={open} dismissed={dismissed} models={models} folders={folders} hint onHintChange={vi.fn()}
      onDismiss={vi.fn()} onRestore={vi.fn()} onBundle={vi.fn().mockResolvedValue(1)} returnFocus={opener} {...handlers}
      onClose={() => { handlers.onClose?.(); setShown(false); }} />}
  </>;
}
const show = (props: Parameters<typeof Harness>[0] = {}) => render(<LanguageProvider><Harness {...props} /></LanguageProvider>);
const submitButton = () => screen.getByRole('button', { name: /^Ausgewählte bündeln/ });

describe('BundleSuggestionsDialog', () => {
  it('says that nothing is bundled automatically and names reason, origin and files', () => {
    show();
    const dialog = screen.getByRole('dialog', { name: de.bundleSuggestTitle });
    expect(dialog).toHaveTextContent('Es wird nie automatisch gebündelt');
    expect(dialog).toHaveTextContent('Gleiche Modell-ID, verschiedene Profile');
    expect(dialog).toHaveTextContent('makerworld · Modell 77');
    expect(dialog).toHaveTextContent('Ähnlicher Name im selben Ordner');
    expect(dialog).toHaveTextContent('Ordner „Figuren“');
    expect(within(dialog).getByText('Schiff Teil 2.3mf')).toBeInTheDocument();
  });

  it('ticks the model id suggestion with its kind known and leaves the name suggestion off', () => {
    show();
    expect(screen.getByRole('checkbox', { name: 'Vorschlag „Lampe“ bündeln' })).toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Vorschlag „Schiff“ bündeln' })).not.toBeChecked();
    expect(screen.getByRole('checkbox', { name: 'Vorschlag „Lampe“ bündeln' })).toHaveFocus();
    expect(submitButton()).toHaveTextContent('(1)');
    expect(submitButton()).toBeEnabled();
  });

  it('asks for the kind of a ticked name suggestion and keeps the button off until it is chosen', () => {
    show();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Vorschlag „Schiff“ bündeln' }));
    expect(screen.getByRole('alert')).toHaveTextContent(de.bundleSuggestKindMissing);
    expect(submitButton()).toBeDisabled();
    const kinds = screen.getAllByRole('combobox', { name: 'Art' });
    fireEvent.change(kinds[1], { target: { value: 'parts' } });
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(submitButton()).toBeEnabled();
    expect(submitButton()).toHaveTextContent('(2)');
  });

  it('keeps the button off for an empty name', () => {
    show();
    fireEvent.change(screen.getAllByRole('textbox', { name: 'Name' })[0], { target: { value: '  ' } });
    expect(submitButton()).toBeDisabled();
  });

  it('bundles the ticked suggestions with the edited name and the chosen kind, in file order', async () => {
    const onBundle = vi.fn().mockResolvedValue(2);
    show({ handlers: { onBundle } });
    fireEvent.change(screen.getAllByRole('textbox', { name: 'Name' })[0], { target: { value: ' Meine Lampe ' } });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Vorschlag „Schiff“ bündeln' }));
    fireEvent.change(screen.getAllByRole('combobox', { name: 'Art' })[1], { target: { value: 'parts' } });
    fireEvent.click(submitButton());
    await waitFor(() => expect(onBundle).toHaveBeenCalledOnce());
    expect(onBundle).toHaveBeenCalledWith([
      { name: 'Meine Lampe', kind: 'variants', fileIds: byReason('sameModelId').fileIds },
      { name: 'Schiff', kind: 'parts', fileIds: byReason('similarName').fileIds },
    ]);
    // The dialog stays open, says what happened and puts the focus on a control that is still there.
    await waitFor(() => expect(screen.getByRole('button', { name: de.bundleSuggestClose })).toHaveFocus());
    expect(screen.getByRole('dialog')).toHaveTextContent('2 Bündel angelegt (4 Modelle).');
  });

  it('shows a failure inside the dialog and stays open', async () => {
    show({ handlers: { onBundle: vi.fn().mockRejectedValue(new Error('boom')) } });
    fireEvent.click(submitButton());
    expect(await screen.findByRole('alert')).toHaveTextContent('boom');
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    expect(submitButton()).toBeEnabled();
  });

  it('remembers "Do not suggest" per group and announces it', () => {
    const onDismiss = vi.fn();
    show({ handlers: { onDismiss } });
    fireEvent.click(screen.getByRole('button', { name: '„Lampe“ nicht mehr vorschlagen' }));
    expect(onDismiss).toHaveBeenCalledWith(byReason('sameModelId').key);
    expect(screen.getByRole('status')).toHaveTextContent('„Lampe“ wird nicht mehr vorgeschlagen.');
  });

  it('lists dismissed groups behind a toggle and can restore one', () => {
    const onRestore = vi.fn();
    show({ open: [byReason('similarName')], dismissed: [byReason('sameModelId')], handlers: { onRestore } });
    const toggle = screen.getByRole('button', { name: 'Nicht mehr vorgeschlagen (1)' });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByRole('button', { name: /wieder vorschlagen/ })).not.toBeInTheDocument();
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    fireEvent.click(screen.getByRole('button', { name: '„Lampe“ wieder vorschlagen' }));
    expect(onRestore).toHaveBeenCalledWith(byReason('sameModelId').key);
  });

  it('says so when nothing is left and starts on "Close"', () => {
    show({ open: [] });
    expect(screen.getByText(de.bundleSuggestNone)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.bundleSuggestClose })).toHaveFocus();
  });

  it('switches the hint off from inside the dialog', () => {
    const onHintChange = vi.fn();
    show({ handlers: { onHintChange } });
    fireEvent.click(screen.getByRole('checkbox', { name: de.bundleSuggestHintSwitch }));
    expect(onHintChange).toHaveBeenCalledWith(false);
  });

  it('closes with Escape and gives the focus back to the opener', async () => {
    const onClose = vi.fn();
    show({ handlers: { onClose } });
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByRole('button', { name: 'opener' })).toHaveFocus());
  });
});
