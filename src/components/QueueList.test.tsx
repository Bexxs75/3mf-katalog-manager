import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { QueueList } from './QueueList';
import { makeModelFile } from '../test/factories';

beforeEach(() => {
  localStorage.setItem('3mf-katalog-language', 'de');
});

function setup(queue = [makeModelFile({ id: 'a', name: 'Rakete.3mf' }), makeModelFile({ id: 'b', name: 'Vase.3mf' })]) {
  const props = { onQueueReorder: vi.fn(), onQueueRemove: vi.fn(), onQueueSelect: vi.fn() };
  render(<LanguageProvider><QueueList queue={queue} {...props} /></LanguageProvider>);
  return props;
}

describe('QueueList', () => {
  it('shows the empty hint without entries', () => {
    setup([]);
    expect(screen.getByText('Keine Modelle in der Warteschlange')).toBeInTheDocument();
  });

  it('renders numbered entries', () => {
    setup();
    expect(screen.getByText('Rakete.3mf')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('selects an entry on click (mouse down and up on the same row)', () => {
    const props = setup();
    fireEvent.mouseDown(screen.getByText('Vase.3mf'));
    fireEvent.mouseUp(document);
    expect(props.onQueueSelect).toHaveBeenCalledWith('b');
    expect(props.onQueueReorder).not.toHaveBeenCalled();
  });

  it('reorders when dragged onto another row', () => {
    const props = setup();
    fireEvent.mouseDown(screen.getByText('Rakete.3mf'));
    fireEvent.mouseEnter(screen.getByText('Vase.3mf').parentElement!);
    fireEvent.mouseUp(document);
    expect(props.onQueueReorder).toHaveBeenCalledWith(['b', 'a']);
  });

  it('removes an entry via ✕ without selecting it', () => {
    const props = setup();
    fireEvent.click(screen.getAllByText('✕')[0]);
    expect(props.onQueueRemove).toHaveBeenCalledWith('a');
    expect(props.onQueueSelect).not.toHaveBeenCalled();
  });
});
