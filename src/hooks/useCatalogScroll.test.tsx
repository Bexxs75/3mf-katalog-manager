import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { useCatalogScroll } from './useCatalogScroll';
function Workspace() {
  const [detail, setDetail] = useState(false);
  const [view, setView] = useState('grid');
  const ref = useCatalogScroll(detail, view, 'model');
  return <><button onClick={() => setDetail(!detail)}>detail</button>
    <button onClick={() => setView(view === 'grid' ? 'list' : 'grid')}>view</button>
    {!detail && <div ref={ref} data-testid="scroll"><div data-model-id="model" /></div>}</>;
}
describe('catalog return scroll', () => {
  it('restores container scrollTop 1200 after detail closes', () => {
    render(<Workspace />); screen.getByTestId('scroll').scrollTop = 1200;
    fireEvent.click(screen.getByText('detail')); fireEvent.click(screen.getByText('detail'));
    expect(screen.getByTestId('scroll').scrollTop).toBe(1200);
  });
  it('invalidates restoration even if the view changes back while detail is open', () => {
    render(<Workspace />); screen.getByTestId('scroll').scrollTop = 1200;
    fireEvent.click(screen.getByText('detail'));
    fireEvent.click(screen.getByText('view')); fireEvent.click(screen.getByText('view'));
    fireEvent.click(screen.getByText('detail'));
    expect(screen.getByTestId('scroll').scrollTop).toBe(0);
  });
  it.each([true, false])('scrolls selected model only when outside viewport: %s', (outside) => {
    const bounds = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      return { top: this.dataset.modelId ? (outside ? 600 : 100) : 0,
        bottom: this.dataset.modelId ? (outside ? 700 : 200) : 500 } as DOMRect;
    });
    const scroll = vi.fn(); const original = HTMLElement.prototype.scrollIntoView;
    HTMLElement.prototype.scrollIntoView = scroll;
    render(<Workspace />); fireEvent.click(screen.getByText('detail')); fireEvent.click(screen.getByText('detail'));
    expect(scroll).toHaveBeenCalledTimes(outside ? 1 : 0);
    HTMLElement.prototype.scrollIntoView = original; bounds.mockRestore();
  });
});
