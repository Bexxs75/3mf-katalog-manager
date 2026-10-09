import { useState } from 'react';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { PlateSelector } from './PlateSelector';
import { useDetailNavigation } from '../hooks/useDetailNavigation';
const plates = [{number: 1, name: null}, {number: 2, name: 'Regalplatte'}];
function Selector({ list = plates }: { list?: typeof plates }) {
  const [selected, setSelected] = useState<number | null>(null);
  return <PlateSelector plates={list} selected={selected} onSelect={setSelected} />;
}
const trigger = () => screen.getByRole('button', {name: /Platte wählen|Platte \d/});
const previous = () => screen.getByRole('button', {name: 'Vorherige Platte'});
const next = () => screen.getByRole('button', {name: 'Nächste Platte'});
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
it('keeps arrows at the boundaries and All explicitly selects null', () => {
  render(<LanguageProvider><Selector /></LanguageProvider>);
  const all = screen.getByRole('button', {name: 'Alle'});
  expect(all).toHaveAttribute('aria-pressed', 'true');
  expect(previous()).toBeDisabled();
  fireEvent.click(next());
  expect(trigger()).toHaveTextContent('Platte 1');
  expect(trigger()).toHaveAttribute('title', 'Platte 1');
  expect(previous()).toBeDisabled();
  expect(all).toHaveAttribute('aria-pressed', 'false');
  fireEvent.click(next());
  expect(trigger()).toHaveAttribute('title', 'Platte 2 · Regalplatte');
  expect(next()).toBeDisabled();
  fireEvent.click(previous());
  expect(trigger()).toHaveTextContent('Platte 1');
  fireEvent.click(all);
  expect(all).toHaveAttribute('aria-pressed', 'true');
  expect(trigger()).toHaveTextContent('Platte wählen');
});
it('opens, toggles, dismisses outside and selects a named option by mouse', () => {
  render(<LanguageProvider><Selector /></LanguageProvider>);
  expect(trigger()).toHaveAttribute('aria-haspopup', 'listbox');
  expect(trigger()).toHaveAttribute('aria-expanded', 'false');
  fireEvent.click(trigger());
  expect(screen.getByRole('listbox')).toHaveFocus();
  expect(trigger()).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getAllByRole('option').every(option => option.getAttribute('aria-selected') === 'false')).toBe(true);
  fireEvent.click(trigger());
  expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  fireEvent.click(trigger());
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  fireEvent.click(trigger());
  fireEvent.click(screen.getByRole('option', {name: 'Platte 2 Regalplatte'}));
  expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  expect(trigger()).toHaveFocus();
  expect(trigger()).toHaveTextContent('Platte 2 · Regalplatte');
  fireEvent.click(trigger());
  expect(screen.getByRole('option', {name: 'Platte 2 Regalplatte'})).toHaveAttribute('aria-selected', 'true');
});
it.each(['ArrowDown', 'ArrowUp'])('opens with %s and navigates active rows without selecting until Enter', key => {
  render(<LanguageProvider><Selector /></LanguageProvider>);
  fireEvent.keyDown(trigger(), {key});
  const list = screen.getByRole('listbox');
  const options = screen.getAllByRole('option');
  fireEvent.keyDown(list, {key: 'Home'});
  expect(list).toHaveAttribute('aria-activedescendant', options[0].id);
  fireEvent.keyDown(list, {key: 'ArrowDown'});
  expect(list).toHaveAttribute('aria-activedescendant', options[1].id);
  expect(screen.getByRole('button', {name: 'Alle'})).toHaveAttribute('aria-pressed', 'true');
  fireEvent.keyDown(list, {key: 'ArrowUp'});
  expect(list).toHaveAttribute('aria-activedescendant', options[0].id);
  fireEvent.keyDown(list, {key: 'End'});
  fireEvent.keyDown(list, {key: 'Enter'});
  expect(trigger()).toHaveTextContent('Platte 2 · Regalplatte');
  expect(trigger()).toHaveFocus();
  fireEvent.keyDown(trigger(), {key: 'ArrowDown'});
  fireEvent.keyDown(screen.getByRole('listbox'), {key: 'Escape'});
  expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  expect(trigger()).toHaveFocus();
});
it('contains every key event so detail navigation cannot react', () => {
  const navigate = vi.fn();
  function Detail() {
    useDetailNavigation(navigate, () => false);
    return <LanguageProvider><Selector /></LanguageProvider>;
  }
  render(<Detail />);
  fireEvent.keyDown(window, {key: 'ArrowRight'});
  expect(navigate).toHaveBeenCalledWith('next');
  navigate.mockClear();
  for (const button of screen.getAllByRole('button')) {
    for (const key of ['ArrowLeft', 'ArrowRight', 'Escape', 'a']) fireEvent.keyDown(button, {key});
  }
  fireEvent.click(trigger());
  for (const key of ['ArrowLeft', 'ArrowRight', 'ArrowDown', 'ArrowUp', 'Home', 'End', 'a', 'Escape']) {
    fireEvent.keyDown(screen.getByRole('listbox'), {key});
  }
  expect(navigate).not.toHaveBeenCalled();
});
it('uses plate numbers rather than positions for callbacks', () => {
  const select = vi.fn();
  render(<LanguageProvider><PlateSelector plates={[{number: 3, name: null}, {number: 8, name: null}]} selected={3} onSelect={select} /></LanguageProvider>);
  fireEvent.click(next());
  expect(select).toHaveBeenLastCalledWith(8);
  fireEvent.click(screen.getByRole('button', {name: 'Alle'}));
  expect(select).toHaveBeenLastCalledWith(null);
});
it('keeps twenty plates in one fixed control row with options in a separate popup', () => {
  render(<LanguageProvider><Selector list={Array.from({length: 20}, (_, i) => ({number: i + 1, name: null}))} /></LanguageProvider>);
  const bar = screen.getByRole('group', {name: 'Druckplatte'});
  expect(bar).toHaveClass('plate-selector', 'flex-nowrap', 'w-full');
  expect(within(bar).getAllByRole('button')).toHaveLength(4);
  expect(screen.queryByRole('option')).not.toBeInTheDocument();
  fireEvent.click(trigger());
  expect(within(bar).getAllByRole('button')).toHaveLength(4);
  expect(screen.getAllByRole('option')).toHaveLength(20);
  expect(screen.getByRole('listbox')).toHaveClass('absolute', 'overflow-y-auto');
});
it.each([{list: []}, {list: plates.slice(0, 1)}])('hides controls without multiple plates', ({list}) => {
  render(<LanguageProvider><PlateSelector plates={list} selected={null} onSelect={() => {}} /></LanguageProvider>);
  expect(screen.queryByRole('group', {name: 'Druckplatte'})).not.toBeInTheDocument();
});
