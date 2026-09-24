import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { ToolsSection, type ToolsSectionProps } from './ToolsSection';
import { makeModelFile } from '../test/factories';

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
});

function setup(over: Partial<ToolsSectionProps> = {}) {
  const props: ToolsSectionProps = {
    queue: [makeModelFile({ id: 'q1', name: 'Rakete.3mf' })],
    onQueueReorder: vi.fn(),
    onQueueRemove: vi.fn(),
    onQueueSelect: vi.fn(),
    queueFilament: null,
    toolView: null,
    onToolViewChange: vi.fn(),
    counts: { recent: 20, new: 3, favorites: 4, duplicateGroups: 1 },
    onOpenCleanup: vi.fn(),
    cleanupScanning: false,
    cleanupError: null,
    ...over,
  };
  render(<LanguageProvider><ToolsSection {...props} /></LanguageProvider>);
  return props;
}

const row = (label: string) => screen.getByRole('button', { name: new RegExp(label) });

describe('ToolsSection', () => {
  it('shows all entries with their counts', () => {
    setup();
    expect(screen.getByText('Werkzeuge')).toBeInTheDocument();
    expect(row('Warteschlange')).toHaveTextContent('1');
    expect(row('Zuletzt angesehen')).toHaveTextContent('20');
    expect(row('Neu hinzugefügt')).toHaveTextContent('3');
    expect(row('Favoriten')).toHaveTextContent('4');
    expect(row('Duplikate')).toHaveTextContent('1');
    expect(row('Aufräum-Vorschläge')).toBeInTheDocument();
  });

  it('leaves filament storage and trash to the left rail', () => {
    setup();
    expect(screen.queryByRole('button', { name: /Filament-Lager/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Papierkorb/ })).not.toBeInTheDocument();
  });

  it('toggles the favorites view', () => {
    const props = setup();
    fireEvent.click(row('Favoriten'));
    expect(props.onToolViewChange).toHaveBeenCalledWith('favorites');
  });

  it('toggles a view on and off', () => {
    const props = setup();
    fireEvent.click(row('Zuletzt angesehen'));
    expect(props.onToolViewChange).toHaveBeenCalledWith('recent');
  });

  it('marks the active view and deselects it on a second click', () => {
    const props = setup({ toolView: 'duplicates' });
    expect(row('Duplikate')).toHaveAttribute('aria-pressed', 'true');
    expect(row('Zuletzt angesehen')).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(row('Duplikate'));
    expect(props.onToolViewChange).toHaveBeenCalledWith(null);
  });

  it('opens the cleanup suggestions', () => {
    const props = setup();
    fireEvent.click(row('Aufräum-Vorschläge'));
    expect(props.onOpenCleanup).toHaveBeenCalled();
  });

  it('disables cleanup while scanning and shows a scan error', () => {
    setup({ cleanupScanning: true, cleanupError: 'Scan fehlgeschlagen' });
    expect(row('Aufräum-Vorschläge')).toBeDisabled();
    expect(screen.getByText('Scan fehlgeschlagen')).toBeInTheDocument();
  });

  it('collapses and expands the queue list', () => {
    setup();
    expect(screen.getByText('Rakete.3mf')).toBeInTheDocument();
    fireEvent.click(row('Warteschlange'));
    expect(screen.queryByText('Rakete.3mf')).not.toBeInTheDocument();
    fireEvent.click(row('Warteschlange'));
    expect(screen.getByText('Rakete.3mf')).toBeInTheDocument();
  });

  it('collapses the whole section via its heading', () => {
    setup();
    fireEvent.click(screen.getByText('Werkzeuge'));
    expect(screen.queryByRole('button', { name: /Duplikate/ })).not.toBeInTheDocument();
  });

  it('exposes the heading as a keyboard-accessible button with aria-expanded', () => {
    setup();
    const heading = screen.getByRole('button', { name: /Werkzeuge/ });
    expect(heading).toHaveAttribute('aria-expanded', 'true');
    fireEvent.click(heading);
    expect(heading).toHaveAttribute('aria-expanded', 'false');
    fireEvent.click(heading);
    expect(heading).toHaveAttribute('aria-expanded', 'true');
  });
});
