import { fireEvent, render, screen, cleanup } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { useModalDialog } from './useModalDialog';

afterEach(cleanup);
function Dialog({ open = true, onClose = vi.fn(), busy = false }) {
  const ref = useModalDialog({ open, onClose, busy });
  return open ? <div ref={ref} role="dialog" aria-modal="true" tabIndex={-1}>
    <button>First</button><button disabled>Disabled</button><button hidden>Hidden</button><button>Last</button>
  </div> : null;
}
it('focuses inside, cycles Tab in both directions and restores the opener', () => {
  const trigger = document.createElement('button');
  document.body.append(trigger); trigger.focus();
  const { rerender } = render(<Dialog />);
  const first = screen.getByText('First'); const last = screen.getByText('Last');
  expect(document.activeElement).toBe(first);
  fireEvent.keyDown(first, { key: 'Tab', shiftKey: true });
  expect(document.activeElement).toBe(last);
  fireEvent.keyDown(last, { key: 'Tab' });
  expect(document.activeElement).toBe(first);
  trigger.focus(); expect(document.activeElement).toBe(first);
  rerender(<Dialog open={false} />);
  expect(document.activeElement).toBe(trigger); trigger.remove();
});
it('closes on Escape except while busy, using the latest callback', () => {
  const close = vi.fn(); const next = vi.fn();
  const { rerender } = render(<Dialog onClose={close} busy />);
  fireEvent.keyDown(screen.getByText('First'), { key: 'Escape' }); expect(close).not.toHaveBeenCalled();
  rerender(<Dialog onClose={next} />);
  fireEvent.keyDown(screen.getByText('First'), { key: 'Escape' }); expect(next).toHaveBeenCalledOnce();
});
it('blocks background clicks and only closes the top dialog', () => {
  const outer = vi.fn(); const inner = vi.fn(); const background = vi.fn();
  render(<><button onClick={background}>Background</button><Dialog onClose={outer} /><Dialog onClose={inner} /></>);
  fireEvent.click(screen.getByText('Background')); expect(background).not.toHaveBeenCalled();
  fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
  expect(inner).toHaveBeenCalledOnce(); expect(outer).not.toHaveBeenCalled();
});

it('honors explicit initial and return focus refs without resetting focus on rerender', () => {
  const opener = document.createElement('button'); document.body.append(opener);
  const returnFocus = { current: opener };
  function Custom({ open }: { open: boolean }) {
    const ref = useModalDialog({ open, onClose: vi.fn(), returnFocus, initialFocus: '[data-safe]' });
    return open ? <div ref={ref} tabIndex={-1}><button>Delete</button><button data-safe>Cancel</button></div> : null;
  }
  const { rerender } = render(<Custom open />);
  expect(screen.getByText('Cancel')).toHaveFocus();
  screen.getByText('Delete').focus(); rerender(<Custom open />);
  expect(screen.getByText('Delete')).toHaveFocus();
  rerender(<Custom open={false} />); expect(opener).toHaveFocus(); opener.remove();
});

it('keeps a dialog with no enabled controls focusable', () => {
  function Empty() {
    const ref = useModalDialog({ open: true, onClose: vi.fn() });
    return <div ref={ref} role="dialog" tabIndex={-1}><button disabled>Wait</button></div>;
  }
  render(<Empty />);
  expect(screen.getByRole('dialog')).toHaveFocus();
  fireEvent.keyDown(document.activeElement!, { key: 'Tab' });
  expect(screen.getByRole('dialog')).toHaveFocus();
});
