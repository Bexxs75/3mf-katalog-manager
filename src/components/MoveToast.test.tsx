import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { LanguageProviderWithDiagnostics as LanguageProvider } from '../test/renderWithDiagnostics';
import { MoveToast } from './MoveToast';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

function renderToast(props: Partial<Parameters<typeof MoveToast>[0]> = {}) {
  const onDone = vi.fn();
  render(
    <LanguageProvider>
      <MoveToast from="Modell" to="Zielordner" onDone={onDone} {...props} />
    </LanguageProvider>,
  );
  return { onDone };
}

describe('MoveToast', () => {
  it('shows the move confirmation without a report link', () => {
    renderToast();
    expect(screen.getByText('Modell')).toBeInTheDocument();
    expect(screen.getByText('Zielordner')).toBeInTheDocument();
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
  });

  it('offers "Report problem" for an unexpected error', () => {
    renderToast({ to: 'x', error: true, unexpected: true });
    expect(screen.getByText('x')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });

  it('does not offer "Report problem" for an expected error', () => {
    renderToast({ to: 'x', error: true, unexpected: false });
    expect(screen.getByText('x')).toBeInTheDocument();
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
  });

  it('translates a structured failure instead of showing its code', () => {
    renderToast({ from: 'Ordner', to: '', error: true, failure: { message: 'folderNameForbiddenChar', params: ['?'], unexpected: false } });
    expect(screen.getByText('Dieses Zeichen ist in Ordnernamen nicht erlaubt: ?')).toBeInTheDocument();
    expect(screen.queryByText('folderNameForbiddenChar')).not.toBeInTheDocument();
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
  });

  it('translates the failure of a refused undo step inside the undo message', () => {
    renderToast({ from: 'A', to: '', error: true, undo: { kind: 'folderMove', ok: false, name: 'A', failure: { message: 'folderMoveNameTaken', params: ['/x/A'], unexpected: false } } });
    expect(screen.getByRole('alert').textContent).toContain('Der Zielordner enthält bereits einen Ordner mit diesem Namen: /x/A');
    expect(screen.getByRole('alert').textContent).not.toContain('folderMoveNameTaken');
  });

  it('shows an unexpected failure as technical text with the report link', () => {
    renderToast({ to: '', error: true, failure: { message: 'DB-Update fehlgeschlagen', unexpected: true } });
    expect(screen.getByText('DB-Update fehlgeschlagen')).toBeInTheDocument();
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });

  it('shows the translated reason for a refused folder drop', () => {
    renderToast({ from: 'Ordner', to: '', error: true, blocked: 'nameTaken' });
    expect(screen.getByText('Der Zielordner enthält bereits einen Ordner mit diesem Namen.')).toBeInTheDocument();
    expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
  });

  it('calls onDone after 3s', () => {
    vi.useFakeTimers();
    const { onDone } = renderToast();
    vi.advanceTimersByTime(3000);
    expect(onDone).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('keeps an error toast for 8s', () => {
    vi.useFakeTimers();
    const { onDone } = renderToast({ error: true, to: 'Fehler' });
    vi.advanceTimersByTime(7999);
    expect(onDone).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(onDone).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it('wraps a long error message instead of truncating it', () => {
    const path = '/sehr/langer/pfad/ohne/leerzeichen/' + 'x'.repeat(200);
    renderToast({ error: true, to: `Der Zielordner existiert nicht mehr: ${path}`, unexpected: true });
    const message = screen.getByText(new RegExp(path.slice(0, 30)));
    const toast = screen.getByRole('status');
    for (const el of [toast, ...Array.from(toast.querySelectorAll('*'))]) {
      expect(el.getAttribute('class') ?? '').not.toMatch(/whitespace-nowrap|text-ellipsis|truncate/);
    }
    expect(message.parentElement?.className).toContain('break-words');
    expect(screen.getByText('Problem melden')).toBeInTheDocument();
  });
});

it.each([
  ['added', '„Modell“ zur Sammlung „Zielordner“ hinzugefügt'],
  ['already', '„Modell“ ist schon in der Sammlung „Zielordner“'],
] as const)('shows collection confirmation %s without a report link', (collection, text) => {
  renderToast({ collection });
  expect(screen.getByRole('status')).toHaveTextContent(text);
  expect(screen.queryByText('Problem melden')).not.toBeInTheDocument();
});

it.each([
  {error: true},
  {collection: 'added' as const},
  {collection: 'already' as const},
])('displays bidi controls visibly in the destination for %j', props => {
  renderToast({...props, to: 'Ziel\u202E.stl'});
  const toast = screen.getByRole('status');
  expect(toast).toHaveTextContent('Ziel[U+202E].stl');
  expect(toast.textContent).not.toContain('\u202E');
});
