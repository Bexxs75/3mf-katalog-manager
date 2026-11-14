import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { BulkCheckbox } from './BulkCheckbox';

it('enlarges only the hit area and keeps selection clicks inside the label', () => {
  const toggle = vi.fn();
  const open = vi.fn();
  render(<div onClick={open}><BulkCheckbox checked={false} onToggle={toggle} /></div>);
  const input = screen.getByRole('checkbox');
  const label = input.closest('label')!;
  expect(label).toHaveClass('w-7', 'h-7', '-m-1.5', 'items-center', 'justify-center');
  expect(input.parentElement).toHaveClass('w-4', 'h-4');
  fireEvent.click(label);
  expect(toggle).toHaveBeenCalledOnce();
  expect(open).not.toHaveBeenCalled();
});
