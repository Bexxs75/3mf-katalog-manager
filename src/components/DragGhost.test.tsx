import { expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { DragGhost } from './DragGhost';

it('shows the name and follows the window pointer without intercepting events', () => {
  render(<DragGhost name="Gear" image="data:image/png;base64,AA" />);
  fireEvent.mouseMove(window, { clientX: 120, clientY: 80 });
  const ghost = screen.getByText('Gear').parentElement;
  expect(ghost).toHaveStyle({ left: '130px', top: '90px', pointerEvents: 'none', position: 'fixed' });
  expect(ghost?.querySelector('img')).toBeTruthy();
});

it('starts at the pointer that triggered dragging', () => {
  render(<DragGhost name="Gear" initialPosition={{ x: 25, y: 40 }} />);
  expect(screen.getByText('Gear').parentElement).toHaveStyle({ left: '35px', top: '50px', visibility: 'visible' });
});
