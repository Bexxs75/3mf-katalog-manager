import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { CollectionsGallery } from './CollectionsGallery';

it('deletes only on confirmation and returns focus after safe dismissal', () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  const onDelete = vi.fn(); const onSelect = vi.fn();
  render(<LanguageProvider><CollectionsGallery collections={[{id:'c1', name:'Kitchen', modelCount:2}]}
    onDelete={onDelete} onSelect={onSelect} onCreate={vi.fn()} onRename={vi.fn()} /></LanguageProvider>);
  const trigger = screen.getByLabelText(/Sammlung wirklich löschen/);
  fireEvent.click(trigger);
  expect(screen.getByRole('dialog')).toHaveAttribute('aria-modal', 'true');
  expect(screen.getByRole('button', {name:'Abbrechen'})).toHaveFocus();
  expect(onDelete).not.toHaveBeenCalled(); expect(onSelect).not.toHaveBeenCalled();
  fireEvent.keyDown(document, {key:'Escape'});
  expect(screen.queryByRole('dialog')).toBeNull(); expect(trigger).toHaveFocus();
  fireEvent.click(trigger);
  fireEvent.click(screen.getByRole('button', {name:'Abbrechen'}));
  expect(onDelete).not.toHaveBeenCalled(); expect(trigger).toHaveFocus();
  fireEvent.click(trigger);
  fireEvent.click(screen.getByRole('button', {name:'Löschen'}));
  expect(onDelete).toHaveBeenCalledExactlyOnceWith('c1');
});
