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
const previous = () => screen.getByRole('button', {name: 'Vorherige Platte'});
const next = () => screen.getByRole('button', {name: 'Nächste Platte'});
const number = (n: number) => screen.getByRole('button', {name: new RegExp(`^Platte ${n}( ·|$)`)});
beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));
it('starts at All, advances to the first plate and stops at both boundaries without wrapping', () => {
  render(<LanguageProvider><Selector /></LanguageProvider>);
  const all = screen.getByRole('button', {name: 'Alle'});
  expect(all).toHaveAttribute('aria-pressed', 'true');
  expect(previous()).toHaveAttribute('aria-disabled', 'true');
  fireEvent.click(previous());
  expect(all).toHaveAttribute('aria-pressed', 'true');
  next().focus();
  fireEvent.click(next());
  expect(number(1)).toHaveAttribute('aria-current', 'true');
  expect(next()).toHaveFocus();
  expect(previous()).toHaveAttribute('aria-disabled', 'true');
  expect(all).toHaveAttribute('aria-pressed', 'false');
  fireEvent.click(next());
  expect(number(2)).toHaveAttribute('aria-current', 'true');
  expect(next()).toHaveAttribute('aria-disabled', 'true');
  expect(next()).toHaveFocus();
  fireEvent.click(next());
  expect(number(2)).toHaveAttribute('aria-current', 'true');
  previous().focus();
  fireEvent.click(previous());
  expect(previous()).toHaveFocus();
  expect(number(1)).toHaveAttribute('aria-current', 'true');
  fireEvent.click(all);
  expect(all).toHaveAttribute('aria-pressed', 'true');
  expect(number(1)).not.toHaveAttribute('aria-current');
  expect(number(2)).not.toHaveAttribute('aria-current');
});
it('selects a numbered button with its full name and tooltip and retains its focus', () => {
  render(<LanguageProvider><Selector /></LanguageProvider>);
  const named = number(2);
  expect(named).toHaveTextContent(/^2$/);
  expect(named).toHaveAttribute('title', 'Platte 2 · Regalplatte');
  expect(named).toHaveAccessibleName('Platte 2 · Regalplatte');
  named.focus();
  fireEvent.click(named);
  expect(named).toHaveFocus();
  expect(named).toHaveAttribute('aria-current', 'true');
  expect(number(1)).not.toHaveAttribute('aria-current');
  expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
  expect(screen.queryByRole('option')).not.toBeInTheDocument();
  expect(screen.getAllByRole('button').map(button => button.getAttribute('aria-label') ?? button.textContent))
    .toEqual(['Alle', 'Vorherige Platte', 'Platte 1', 'Platte 2 · Regalplatte', 'Nächste Platte']);
  expect(previous().querySelector('svg')).not.toBeNull();
  expect(next().querySelector('svg')).not.toBeNull();
});
it('uses Left and Right within the group without moving focus or wrapping', () => {
  render(<LanguageProvider><Selector /></LanguageProvider>);
  const all = screen.getByRole('button', {name: 'Alle'});
  all.focus();
  fireEvent.keyDown(all, {key: 'ArrowLeft'});
  expect(all).toHaveAttribute('aria-pressed', 'true');
  fireEvent.keyDown(all, {key: 'ArrowRight'});
  expect(number(1)).toHaveAttribute('aria-current', 'true');
  fireEvent.keyDown(all, {key: 'ArrowRight'});
  fireEvent.keyDown(all, {key: 'ArrowRight'});
  expect(number(2)).toHaveAttribute('aria-current', 'true');
  fireEvent.keyDown(all, {key: 'ArrowLeft'});
  fireEvent.keyDown(all, {key: 'ArrowLeft'});
  expect(number(1)).toHaveAttribute('aria-current', 'true');
  expect(all).toHaveFocus();
});
it('announces All and the current named plate in a polite hidden live region', () => {
  render(<LanguageProvider><Selector /></LanguageProvider>);
  const live = screen.getByRole('status');
  expect(live).toHaveAttribute('aria-live', 'polite');
  expect(live).toHaveClass('sr-only');
  expect(live).toHaveTextContent('Alle Platten');
  fireEvent.click(number(2));
  expect(live).toHaveTextContent('Platte 2 · Regalplatte');
  fireEvent.click(screen.getByRole('button', {name: 'Alle'}));
  expect(live).toHaveTextContent('Alle Platten');
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
    for (const key of ['ArrowLeft', 'ArrowRight', 'ArrowDown', 'ArrowUp', 'Home', 'End', 'Escape', 'a']) {
      fireEvent.keyDown(button, {key});
    }
  }
  expect(navigate).not.toHaveBeenCalled();
});
it('uses plate numbers rather than positions for number and arrow callbacks', () => {
  const select = vi.fn();
  render(<LanguageProvider><PlateSelector plates={[{number: 3, name: null}, {number: 8, name: null}]} selected={3} onSelect={select} /></LanguageProvider>);
  fireEvent.click(next());
  expect(select).toHaveBeenLastCalledWith(8);
  fireEvent.click(number(8));
  expect(select).toHaveBeenLastCalledWith(8);
  fireEvent.click(screen.getByRole('button', {name: 'Alle'}));
  expect(select).toHaveBeenLastCalledWith(null);
});
it('shows all twelve numbers at the threshold', () => {
  render(<LanguageProvider><Selector list={Array.from({length: 12}, (_, i) => ({number: i + 1, name: null}))} /></LanguageProvider>);
  expect(screen.getAllByRole('button')).toHaveLength(15);
  expect(number(12)).toHaveTextContent('12');
});
it('replaces numbers above twelve plates with a counter and a named plate tooltip', () => {
  render(<LanguageProvider><Selector list={Array.from({length: 24}, (_, i) => ({number: i + 1, name: i === 6 ? 'Regalplatte' : null}))} /></LanguageProvider>);
  const bar = screen.getByRole('group', {name: 'Druckplatte'});
  expect(within(bar).getAllByRole('button')).toHaveLength(3);
  expect(bar).not.toHaveClass('w-full');
  expect(bar).toHaveClass('min-w-0');
  for (let i = 0; i < 7; i++) fireEvent.click(next());
  expect(screen.getByText('7 / 24')).toBeInTheDocument();
  expect(screen.getByText('Regalplatte')).toHaveAttribute('title', 'Regalplatte');
  expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
});
it.each([{list: []}, {list: plates.slice(0, 1)}])('hides controls without multiple plates', ({list}) => {
  render(<LanguageProvider><PlateSelector plates={list} selected={null} onSelect={() => {}} /></LanguageProvider>);
  expect(screen.queryByRole('group', {name: 'Druckplatte'})).not.toBeInTheDocument();
});
