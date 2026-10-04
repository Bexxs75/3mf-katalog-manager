import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { PrinterLinkControl } from './PrinterLinkControl';
import type { PrinterLinkState } from '../hooks/usePrinterLink';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

function link(over: Partial<PrinterLinkState> = {}): PrinterLinkState {
  return {
    enabled: false, connections: [], jobs: [], error: null,
    refresh: vi.fn(), setEnabled: vi.fn().mockResolvedValue(undefined), testConnection: vi.fn(),
    removeConnection: vi.fn(), syncNow: vi.fn(), ignoreJob: vi.fn(), confirmJobs: vi.fn(), previewJob: vi.fn(),
    ...over,
  } as PrinterLinkState;
}

describe('PrinterLinkControl', () => {
  it('switches the printer connection', () => {
    const l = link();
    render(<LanguageProvider><PrinterLinkControl link={l} /></LanguageProvider>);
    fireEvent.click(screen.getByRole('switch', { name: 'Anbindung' }));
    expect(l.setEnabled).toHaveBeenCalledWith(true);
  });

  it('shows the sync interval and expands the explanation', () => {
    render(<LanguageProvider><PrinterLinkControl link={link({ enabled: true })} /></LanguageProvider>);
    expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('Abgleich alle 5 Minuten')).toBeInTheDocument();
    const more = screen.getByRole('button', { name: 'Was passiert dabei?' });
    expect(more).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText(/Keine Cloud/)).toBeNull();
    fireEvent.click(more);
    expect(more).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText(/Keine Cloud/)).toBeInTheDocument();
    fireEvent.click(more);
    expect(screen.queryByText(/Keine Cloud/)).toBeNull();
    expect(screen.queryByRole('list')).toBeNull();
  });

  it('shows the disabled caption', () => {
    render(<LanguageProvider><PrinterLinkControl link={link()} /></LanguageProvider>);
    expect(screen.getByText('Ausgeschaltet')).toBeInTheDocument();
    expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false');
  });

  it('shows an error message when switching the toggle is rejected', async () => {
    const l = link({ setEnabled: vi.fn().mockRejectedValue('offline') });
    render(<LanguageProvider><PrinterLinkControl link={l} /></LanguageProvider>);
    fireEvent.click(screen.getByRole('switch', { name: 'Anbindung' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Das hat nicht geklappt: offline');
  });

  it('shows the background loading error from the printer link', () => {
    const l = link({ error: { message: 'Netzwerk kaputt', unexpected: true } });
    render(<LanguageProvider><PrinterLinkControl link={l} /></LanguageProvider>);
    expect(screen.getByRole('alert')).toHaveTextContent('Das hat nicht geklappt: Netzwerk kaputt');
  });

});
