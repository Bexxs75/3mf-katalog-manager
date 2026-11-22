import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ContextMenuFrame } from './ContextMenu';
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
it('keeps a margin to the viewport edges', () => {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({ width: 172, height: 300, top: 0, left: 0, right: 172, bottom: 300, x: 0, y: 0, toJSON: () => ({}) });
  Object.assign(window, { innerWidth: 1000, innerHeight: 600 });
  render(<ContextMenuFrame x={990} y={590} onClose={vi.fn()}><button>a</button></ContextMenuFrame>);
  const frame = screen.getByRole('button').parentElement!;
  expect(frame.style.left).toBe('820px');
  expect(frame.style.top).toBe('292px');
});
