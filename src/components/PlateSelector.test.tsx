import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PlateSelector } from './PlateSelector';
const plates = [{number: 1, name: null}, {number: 2, name: 'Regalplatte'}];
function Selector() {
  const [selected, setSelected] = useState<number | null>(null);
  return <PlateSelector plates={plates} selected={selected} onSelect={setSelected} />;
}
it('has one tab stop, named plates and wrapping arrow navigation', () => {
  localStorage.setItem('3mf-katalog-language', 'de');
  render(<LanguageProvider><Selector /></LanguageProvider>);
  expect(screen.getByRole('radiogroup', {name: 'Druckplatte'})).toBeVisible();
  const all = screen.getByRole('radio', {name: 'Alle'});
  const first = screen.getByRole('radio', {name: 'Platte 1'});
  const last = screen.getByRole('radio', {name: 'Platte 2'});
  expect(all).toHaveAttribute('tabindex', '0');
  expect(first).toHaveAttribute('tabindex', '-1');
  expect(last).toHaveAttribute('title', 'Regalplatte');
  fireEvent.keyDown(all, {key: 'ArrowRight'});
  expect(first).toHaveFocus(); expect(first).toHaveAttribute('aria-checked', 'true');
  fireEvent.keyDown(first, {key: 'ArrowDown'});
  expect(last).toHaveFocus();
  fireEvent.keyDown(last, {key: 'ArrowRight'});
  expect(all).toHaveFocus();
  fireEvent.keyDown(all, {key: 'ArrowLeft'});
  expect(last).toHaveFocus();
  fireEvent.keyDown(last, {key: 'Home'});
  expect(all).toHaveFocus();
  fireEvent.keyDown(all, {key: 'End'});
  expect(last).toHaveFocus();
});
it.each([{list: []}, {list: plates.slice(0, 1)}])('hides controls without multiple plates', ({list}) => {
  render(<LanguageProvider><PlateSelector plates={list} selected={null} onSelect={() => {}} /></LanguageProvider>);
  expect(screen.queryByRole('radiogroup')).not.toBeInTheDocument();
});
