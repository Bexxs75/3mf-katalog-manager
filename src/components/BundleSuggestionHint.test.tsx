import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { BundleSuggestionHint } from './BundleSuggestionHint';

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

const show = (count: number, onView = vi.fn(), onClose = vi.fn()) => {
  render(<LanguageProvider><BundleSuggestionHint count={count} onView={onView} onClose={onClose} /></LanguageProvider>);
  return { onView, onClose };
};

describe('BundleSuggestionHint', () => {
  it('is not modal and states the number and that nothing happens by itself', () => {
    show(5);
    expect(screen.getByRole('region', { name: 'Hinweis zu Bündel-Vorschlägen' })).toHaveTextContent('5 Vorschläge zum Bündeln gefunden. Es wird nie automatisch gebündelt.');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('uses the singular', () => {
    show(1);
    expect(screen.getByRole('status')).toHaveTextContent('1 Vorschlag zum Bündeln gefunden.');
  });

  it('renders nothing without suggestions', () => {
    show(0);
    expect(screen.queryByRole('region')).not.toBeInTheDocument();
  });

  it('opens the dialog from its own button and closes with "Später"', () => {
    const { onView, onClose } = show(2);
    const view = screen.getByRole('button', { name: 'Ansehen' });
    fireEvent.click(view);
    expect(onView).toHaveBeenCalledWith(view);
    fireEvent.click(screen.getByRole('button', { name: 'Später' }));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
