import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { DetailPanelResizer } from './DetailPanelResizer';
import { useDetailPanelWidth } from '../hooks/useDetailPanelWidth';
import { LanguageProvider } from '../i18n/LanguageContext';
const key = '3mf-katalog-detail-width';
function setup() {
  function Fixture() {
    const { width, setWidth, reset } = useDetailPanelWidth();
    return <DetailPanelResizer width={width ?? 336} workspaceWidth={1000} setWidth={setWidth} reset={reset} />;
  }
  render(<LanguageProvider><Fixture /></LanguageProvider>);
  return screen.getByRole('separator');
}
beforeEach(() => localStorage.clear());
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
it('exposes an accessible, focusable vertical separator', () => {
  const separator = setup();
  expect(separator).toHaveAttribute('aria-orientation', 'vertical');
  expect(separator).toHaveAttribute('tabindex', '0');
  expect(separator).toHaveAccessibleName();
  expect(separator).toHaveAttribute('aria-valuenow', '336');
  expect(separator).toHaveAttribute('aria-valuemin', '300');
  expect(separator).toHaveAttribute('aria-valuemax', '600');
});
it('handles arrows, shift, limits and both reset gestures', () => {
  const separator = setup();
  const press = (key: string, expected: number, shiftKey = false) => {
    fireEvent.keyDown(separator, { key, shiftKey });
    expect(separator).toHaveAttribute('aria-valuenow', String(expected));
    expect(localStorage.getItem('3mf-katalog-detail-width')).toBe(String(expected));
  };
  press('ArrowLeft', 352); press('ArrowRight', 336);
  press('ArrowLeft', 400, true); press('ArrowRight', 336, true);
  press('Home', 300); press('ArrowRight', 300); press('End', 600); press('ArrowLeft', 600);
  fireEvent.keyDown(separator, { key: 'Enter' });
  expect(localStorage.getItem(key)).toBeNull(); expect(separator).toHaveAttribute('aria-valuenow', '336');
  press('End', 600); fireEvent.doubleClick(separator);
  expect(localStorage.getItem(key)).toBeNull(); expect(separator).toHaveAttribute('aria-valuenow', '336');
});
it('captures the pointer, previews without saving, and saves the bounded width on release', () => {
  vi.stubGlobal('PointerEvent', MouseEvent);
  const separator = setup();
  const capture = vi.fn(); const release = vi.fn();
  separator.setPointerCapture = capture; separator.releasePointerCapture = release;
  fireEvent.pointerDown(separator, { button: 0, clientX: 700 });
  expect(capture).toHaveBeenCalled();
  fireEvent.pointerMove(separator, { clientX: 600 });
  expect(separator).toHaveAttribute('aria-valuenow', '436');
  expect(localStorage.getItem(key)).toBeNull();
  fireEvent.pointerMove(separator, { clientX: 0 });
  expect(separator).toHaveAttribute('aria-valuenow', '600');
  fireEvent.pointerUp(separator, { clientX: 0 });
  expect(localStorage.getItem(key)).toBe('600'); expect(release).toHaveBeenCalled();
});
