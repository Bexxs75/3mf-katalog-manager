import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { TagInput } from './TagInput';

function setup(allTags = ['Zebra', 'Gehäuse', 'Gehäuse groß', 'bereits'], tags = ['bereits']) {
  const onAddTag = vi.fn();
  const parent = vi.fn();
  render(<LanguageProvider><div onKeyDown={parent}><TagInput allTags={allTags} tags={tags} onAddTag={onAddTag} /></div></LanguageProvider>);
  return { input: screen.getByRole('combobox'), onAddTag, parent };
}

afterEach(() => localStorage.clear());

describe('TagInput', () => {
  it('filters case-insensitively, preserves umlauts, sorts and excludes assigned tags', () => {
    const { input } = setup();
    fireEvent.focus(input);
    expect(screen.getAllByRole('option').map(o => o.textContent)).toEqual(['Gehäuse', 'Gehäuse groß', 'Zebra']);
    fireEvent.change(input, { target: { value: 'GEH' } });
    expect(screen.getAllByRole('option').map(o => o.textContent)).toEqual(['Gehäuse', 'Gehäuse groß']);
    fireEvent.change(input, { target: { value: 'HÄU' } });
    expect(screen.getAllByRole('option')).toHaveLength(2);
  });
  it('selects with arrows and Enter and clears the draft', () => {
    const { input, onAddTag } = setup();
    fireEvent.change(input, { target: { value: 'geh' } });
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAddTag).toHaveBeenCalledWith('Gehäuse');
    expect(input).toHaveValue('');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  });
  it('submits the typed value without an explicitly highlighted suggestion', () => {
    const { input, onAddTag } = setup();
    fireEvent.change(input, { target: { value: 'geh' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAddTag).toHaveBeenCalledWith('geh');
  });
  it('selects by mouse', () => {
    const { input, onAddTag } = setup();
    fireEvent.change(input, { target: { value: 'geh' } });
    fireEvent.mouseDown(screen.getByRole('option', { name: 'Gehäuse groß' }));
    expect(onAddTag).toHaveBeenCalledWith('Gehäuse groß');
  });
  it('consumes only the first Escape and keeps the draft; Tab closes without submitting', () => {
    const { input, onAddTag, parent } = setup();
    fireEvent.change(input, { target: { value: 'geh' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(input).toHaveValue('geh');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    expect(parent).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(parent).toHaveBeenCalledOnce();
    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: 'Tab' });
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    expect(onAddTag).not.toHaveBeenCalled();
  });
  it('limits suggestions to eight and excludes canonical aliases', () => {
    const { input, onAddTag } = setup(['Multipart', ...Array.from({ length: 12 }, (_, i) => `Tag ${i}`)], ['mehrteilig']);
    fireEvent.focus(input);
    expect(screen.getAllByRole('option')).toHaveLength(8);
    expect(screen.queryByRole('option', { name: 'Multipart' })).not.toBeInTheDocument();
    fireEvent.change(input, { target: { value: 'Multipart' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAddTag).not.toHaveBeenCalled();
  });
});

it('sorts by translated labels and submits the stored canonical label', () => {
  localStorage.setItem('3mf-katalog-language', 'en');
  const { input, onAddTag } = setup(['mehrfarbig', 'miniatur', 'mehrteilig'], []);
  fireEvent.focus(input);
  expect(screen.getByRole('listbox', { name: 'Tag suggestions' })).toBeInTheDocument();
  expect(screen.getAllByRole('option').map(o => o.textContent)).toEqual(['mini', 'multicolor', 'multipart']);
  fireEvent.keyDown(input, { key: 'ArrowUp' });
  const selected = screen.getByRole('option', { selected: true });
  expect(input).toHaveAttribute('aria-activedescendant', selected.id);
  fireEvent.keyDown(input, { key: 'Enter' });
  expect(onAddTag).toHaveBeenCalledWith('mehrteilig');
});
