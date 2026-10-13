import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ViewportPopover } from './ViewportPopover';
import { AutocompleteInput } from './AutocompleteInput';
import { popoverShift } from '../lib/popoverShift';

let left: number;
let width: number;
let top: number;
let height: number;
let anchorLeft: number;
let frames: Map<number, FrameRequestCallback>;
let nextFrame: number;
let observers: ResizeObserverMock[];

class ResizeObserverMock {
  targets = new Set<Element>();
  constructor(private callback: ResizeObserverCallback) { observers.push(this); }
  observe = (target: Element) => { this.targets.add(target); };
  unobserve = (target: Element) => { this.targets.delete(target); };
  disconnect = () => { this.targets.clear(); };
  notify(target: Element) {
    if (this.targets.has(target)) this.callback([], this as unknown as ResizeObserver);
  }
}

function resize(target: Element) {
  act(() => observers.forEach(observer => observer.notify(target)));
}

function frame() {
  act(() => {
    const pending = [...frames.values()];
    frames.clear();
    pending.forEach(callback => callback(0));
  });
}

function mount(children = <span>Content</span>) {
  return render(<div data-testid="toolbar"><div data-testid="anchor">
    <button>Anchor</button><ViewportPopover data-testid="popover">{children}</ViewportPopover>
  </div></div>);
}

beforeEach(() => {
  left = 850;
  width = 250;
  top = 40;
  height = 40;
  anchorLeft = 850;
  frames = new Map();
  nextFrame = 0;
  observers = [];
  vi.stubGlobal('ResizeObserver', ResizeObserverMock);
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    frames.set(++nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
  vi.spyOn(document.documentElement, 'clientWidth', 'get').mockReturnValue(1000);
  vi.spyOn(window, 'scrollX', 'get').mockReturnValue(0);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    const popover = this.dataset.testid === 'popover';
    const dx = Number(this.style.transform.match(/translateX\(([-\d.]+)px\)/)?.[1] ?? 0);
    return new DOMRect(popover ? left + dx : anchorLeft, top, popover ? width : 100, height);
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('ViewportPopover', () => {
  it('sets the calculated transform and always measures the unshifted box', () => {
    mount();
    const popover = screen.getByTestId('popover');
    expect(popover.style.transform).toBe('translateX(-108px)');
    expect(popover.style.transform).toBe(`translateX(${popoverShift(left, width, 1000, 8)}px)`);
    for (let i = 0; i < 3; i++) fireEvent(window, new Event('resize'));
    expect(popover.style.transform).toBe('translateX(-108px)');
    expect(popover.getBoundingClientRect().right).toBe(992);
  });

  it('writes styles on animation frames only when left or width changes', () => {
    mount();
    const popover = screen.getByTestId('popover');
    const transform = vi.spyOn(popover.style, 'transform', 'set');
    frame();
    frame();
    expect(transform).not.toHaveBeenCalled();
    top = 80;
    height = 90;
    frame();
    expect(transform).not.toHaveBeenCalled();
    left = anchorLeft = 940;
    frame();
    expect(transform).toHaveBeenCalled();
    expect(popover.style.transform).toBe('translateX(-198px)');
    transform.mockClear();
    width = 300;
    frame();
    expect(transform).toHaveBeenCalled();
    expect(popover.style.transform).toBe('translateX(-248px)');
    transform.mockClear();
    frame();
    expect(transform).not.toHaveBeenCalled();
  });

  it('repositions after toolbar resizing moves the anchor without a window resize', () => {
    mount();
    left = anchorLeft = 940;
    resize(screen.getByTestId('toolbar'));
    expect(screen.getByTestId('popover').style.transform).toBe('translateX(-198px)');
  });

  it('repositions when only the anchor position changes, without any size change', () => {
    mount();
    left = anchorLeft = 940;
    frame();
    expect(screen.getByTestId('popover').style.transform).toBe('translateX(-198px)');
  });

  it('repositions on anchor or content resizing and removes an unnecessary offset', () => {
    mount();
    const popover = screen.getByTestId('popover');
    width = 350;
    resize(popover);
    expect(popover.style.transform).toBe('translateX(-208px)');
    left = anchorLeft = 100;
    resize(screen.getByTestId('anchor'));
    expect(popover.style.transform).toBe('');
  });

  it('uses viewport coordinates even when the document is horizontally scrolled', () => {
    vi.spyOn(window, 'scrollX', 'get').mockReturnValue(100);
    left = 20;
    width = 950;
    mount();
    const popover = screen.getByTestId('popover');
    expect(popover.style.transform).toBe('');
    expect(popover.getBoundingClientRect().left).toBeGreaterThanOrEqual(8);
    expect(popover.getBoundingClientRect().right).toBeLessThanOrEqual(992);
  });

  it('uses clientWidth rather than the viewport width including a scrollbar', () => {
    vi.stubGlobal('innerWidth', 1020);
    mount();
    expect(screen.getByTestId('popover').getBoundingClientRect().right).toBe(992);
  });

  it('keeps the left edge of an oversized popover visible', () => {
    width = 1200;
    mount();
    expect(screen.getByTestId('popover').getBoundingClientRect().left).toBe(8);
  });

  it('does not let autofocus scroll the document or the clipping app ancestor', () => {
    const nativeFocus = HTMLElement.prototype.focus;
    vi.spyOn(HTMLElement.prototype, 'focus').mockImplementation(function (this: HTMLElement, options?: FocusOptions) {
      if (!options?.preventScroll) {
        document.documentElement.scrollLeft = 100;
        this.closest('[data-testid="toolbar"]')!.scrollLeft = 100;
      }
      nativeFocus.call(this, options);
    });
    document.documentElement.scrollLeft = 0;
    mount(<AutocompleteInput value="" onChange={vi.fn()} options={['Tag']} autoFocus />);
    expect(document.activeElement).toBe(screen.getByRole('textbox'));
    expect(document.documentElement.scrollLeft).toBe(0);
    expect(screen.getByTestId('toolbar').scrollLeft).toBe(0);
    expect(screen.getByTestId('popover').getBoundingClientRect().right).toBe(992);
    document.documentElement.scrollLeft = 0;
  });

  it('disconnects observers, removes the resize listener and cancels tracking on unmount', () => {
    const add = vi.spyOn(window, 'addEventListener');
    const remove = vi.spyOn(window, 'removeEventListener');
    const { unmount } = mount();
    const popover = screen.getByTestId('popover');
    expect(observers.some(observer => observer.targets.has(popover))).toBe(true);
    const listener = add.mock.calls.find(([type]) => type === 'resize')![1];
    unmount();
    expect(observers.every(observer => observer.targets.size === 0)).toBe(true);
    expect(remove).toHaveBeenCalledWith('resize', listener);
    expect(frames.size).toBe(0);
    const measure = vi.spyOn(popover, 'getBoundingClientRect').mockClear();
    fireEvent(window, new Event('resize'));
    resize(popover);
    frame();
    expect(measure).not.toHaveBeenCalled();
  });
});
