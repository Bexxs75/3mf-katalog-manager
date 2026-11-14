import { useRef, useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { BundleDeleteChoiceDialog } from './BundleDeleteChoiceDialog';
import { ImportLockContext } from '../hooks/ImportLockContext';
import { de } from '../i18n/de';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function setup(extra: Partial<Parameters<typeof BundleDeleteChoiceDialog>[0]> = {}, jobActive = false) {
  const props = { onDissolve: vi.fn().mockResolvedValue(undefined), onTrash: vi.fn().mockResolvedValue(undefined), onClose: vi.fn() };
  function Host() {
    const opener = useRef<HTMLButtonElement>(null);
    const [shown, setShown] = useState(true);
    return <>
      <button ref={opener}>opener</button>
      {shown && <BundleDeleteChoiceDialog bundleNames={['Hafen']} fileNames={['Eins.3mf', 'Zwei.stl', 'Drei.3mf']} returnFocus={opener}
        {...props} {...extra} onClose={() => { props.onClose(); setShown(false); }} />}
    </>;
  }
  render(<LanguageProvider><ImportLockContext.Provider value={jobActive}><Host /></ImportLockContext.Provider></LanguageProvider>);
  return props;
}

describe('BundleDeleteChoiceDialog', () => {
  it('asks for the bundle by name, names the count, lists the files and starts on "dissolve only"', () => {
    setup();
    const dialog = screen.getByRole('dialog', { name: 'Bündel „Hafen“ löschen?' });
    expect(dialog).toHaveTextContent('Die Auswahl hat 3 Modelle. Was soll passieren?');
    const dissolve = screen.getByRole('radio', { name: /Nur Bündel auflösen/ });
    expect(dissolve).toBeChecked();
    expect(dissolve).toHaveFocus();
    expect(screen.getByRole('radio', { name: /Alle 3 Modelle in den Papierkorb/ })).not.toBeChecked();
    expect(screen.getByText('Zwei.stl')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: de.bundleDissolve })).toBeInTheDocument();
  });

  it('dissolves by default and never touches the trash', async () => {
    const props = setup();
    fireEvent.click(screen.getByRole('button', { name: de.bundleDissolve }));
    await waitFor(() => expect(props.onClose).toHaveBeenCalled());
    expect(props.onDissolve).toHaveBeenCalledOnce();
    expect(props.onTrash).not.toHaveBeenCalled();
  });

  it('trashes only after the second choice was picked, and the button says so', async () => {
    const props = setup();
    fireEvent.click(screen.getByRole('radio', { name: /Alle 3 Modelle in den Papierkorb/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Alle 3 Modelle in den Papierkorb' }));
    await waitFor(() => expect(props.onTrash).toHaveBeenCalledOnce());
    expect(props.onDissolve).not.toHaveBeenCalled();
  });

  it('says how many bundles when the selection holds several', () => {
    setup({ bundleNames: ['A', 'B'] });
    expect(screen.getByRole('dialog', { name: '2 Bündel löschen?' })).toBeInTheDocument();
  });

  it('shows a failure and stays open', async () => {
    const props = setup({ onDissolve: vi.fn().mockRejectedValue(new Error('kaputt')) });
    fireEvent.click(screen.getByRole('button', { name: de.bundleDissolve }));
    expect(await screen.findByText(/kaputt/)).toBeInTheDocument();
    expect(props.onClose).not.toHaveBeenCalled();
  });

  it('blocks the trash choice during an import but not the harmless one', () => {
    setup({}, true);
    expect(screen.getByRole('button', { name: de.bundleDissolve })).toBeEnabled();
    fireEvent.click(screen.getByRole('radio', { name: /Alle 3 Modelle in den Papierkorb/ }));
    expect(screen.getByRole('button', { name: 'Alle 3 Modelle in den Papierkorb' })).toBeDisabled();
  });

  it('returns the focus to the opener on Escape', () => {
    const props = setup();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(props.onClose).toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'opener' })).toHaveFocus();
  });
});
