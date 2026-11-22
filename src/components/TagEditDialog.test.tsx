import { ImportLockContext } from '../hooks/ImportLockContext';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { TagEditDialog } from './TagEditDialog';
import { LanguageProvider } from '../i18n/LanguageContext';
import { listAllFileTags } from '../lib/api/files';
vi.mock('../lib/api/files', () => ({ listAllFileTags: vi.fn().mockResolvedValue({ a: ['source'], b: ['source', 'target'] }) }));
afterEach(cleanup);
function setup(mode: 'rename' | 'merge', submit = vi.fn().mockResolvedValue(2)) {
  localStorage.setItem('3mf-katalog-language', 'de');
  const done = vi.fn(), close = vi.fn();
  render(<LanguageProvider><TagEditDialog source="source" mode={mode}
    tags={['source', 'target'].map(label => ({label, count: 2, colorHue: 0}))}
    returnFocus={{current: null}} onSubmit={submit} onDone={done} onClose={close} /></LanguageProvider>);
  return {submit, done, close};
}
it('validates names, submits with Enter, shows errors and permits retry', async () => {
  const submit = vi.fn().mockRejectedValueOnce({message: 'tagNameExists', expected: true}).mockResolvedValue(2);
  const {done} = setup('rename', submit);
  const input = screen.getByRole('textbox');
  expect(input).toHaveFocus();
  expect(screen.getByRole('button', {name: 'Umbenennen'})).toBeDisabled();
  fireEvent.change(input, {target: {value: 'target'}});
  expect(screen.getByRole('alert')).toHaveTextContent('Tag existiert');
  fireEvent.change(input, {target: {value: 'new'}});
  await act(async () => fireEvent.submit(input.closest('form')!));
  expect(screen.getByRole('alert')).toHaveTextContent('Tag existiert');
  expect(done).not.toHaveBeenCalled();
  await act(async () => fireEvent.submit(input.closest('form')!));
  expect(submit).toHaveBeenLastCalledWith('source', 'new');
  expect(done).toHaveBeenCalledWith('new');
});
it('uses Select and asks for confirmation with all assigned models before merging', async () => {
  const {submit, done} = setup('merge');
  await act(async () => {});
  expect(listAllFileTags).toHaveBeenCalled();
  fireEvent.click(screen.getByRole('combobox'));
  expect(screen.queryByRole('option', {name: 'source'})).toBeNull();
  fireEvent.click(screen.getByRole('option', {name: 'target'}));
  await act(async () => fireEvent.click(screen.getByRole('button', {name: 'Zusammenführen'})));
  expect(screen.getByText(/2 Modelle von/)).toBeVisible();
  expect(submit).not.toHaveBeenCalled();
  await act(async () => fireEvent.click(screen.getByRole('button', {name: 'Zusammenführen'})));
  expect(submit).toHaveBeenCalledWith('source', 'target');
  expect(done).toHaveBeenCalledWith('target');
});
it('closes with Escape', () => {
  const {close} = setup('rename');
  fireEvent.keyDown(screen.getByRole('textbox'), {key: 'Escape'});
  expect(close).toHaveBeenCalled();
});

it('disables tag changes during imports', () => {
  const submit = vi.fn();
  render(<LanguageProvider><ImportLockContext.Provider value={true}><TagEditDialog
    source="source" mode="rename" tags={[]} onSubmit={submit} onDone={vi.fn()} onClose={vi.fn()} returnFocus={{current: null}} />
  </ImportLockContext.Provider></LanguageProvider>);
  expect(screen.getByRole('textbox')).toBeDisabled();
  expect(screen.getByRole('button', {name: 'Umbenennen'})).toBeDisabled();
  expect(submit).not.toHaveBeenCalled();
});

it('shows no validation hint before the first change', () => {
  setup('rename');
  expect(screen.queryByRole('alert')).toBeNull();
  expect(screen.getByRole('button', {name: 'Umbenennen'})).toBeDisabled();
  fireEvent.change(screen.getByRole('textbox'), {target: {value: 'source '}});
  fireEvent.change(screen.getByRole('textbox'), {target: {value: 'source'}});
  expect(screen.getByRole('alert')).toHaveTextContent('Der neue Name muss sich vom alten unterscheiden.');
});
it('merge dialog shows no hint on open and gives the target select a full-width field', () => {
  setup('merge');
  expect(screen.queryByRole('alert')).toBeNull();
  expect(screen.getByRole('combobox')).toHaveClass('w-full', 'border');
});
