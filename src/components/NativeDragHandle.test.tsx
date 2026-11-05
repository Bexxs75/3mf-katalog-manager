import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import type { ModelFile } from '../types';
const start = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock('../lib/nativeDrag', () => ({ useNativeDragEnabled: () => true, loadNativeDragEnabled: async () => {}, startNativeDrag: start }));
import { NativeDragHandle } from './NativeDragHandle';
const model = {id:'1',path:'/catalog/a.stl',name:'a.stl'} as ModelFile;
beforeEach(() => start.mockClear());
function view(file = model, container = false, parent = vi.fn()) {
  return render(<LanguageProvider><RuntimeEnvironmentProvider value={{container}}><div onMouseDown={parent}><NativeDragHandle model={file} selectedIds={new Set(['1','2'])} /></div></RuntimeEnvironmentProvider></LanguageProvider>);
}
it('starts the selected IDs on mouse down and does not start folder drag', () => {
  const parent = vi.fn(); view(model, false, parent);
  fireEvent.mouseDown(screen.getByRole('button'), {button:0});
  expect(start).toHaveBeenCalledWith(['1','2']);
  expect(parent).not.toHaveBeenCalled();
});
it('does not start on keyboard click or right mouse button', () => {
  view(); const button = screen.getByRole('button');
  fireEvent.click(button); fireEvent.mouseDown(button, {button:2});
  expect(start).not.toHaveBeenCalled();
});
it('omits the grip for STEP and displays the STL alternative', () => {
  view({...model,path:'/catalog/a.step'});
  expect(screen.queryByRole('button')).toBeNull();
  expect(screen.getByText(/STEP.*STL/)).toBeInTheDocument();
});
it('has no native action in containers', () => {
  view(model, true); expect(screen.queryByRole('button')).toBeNull();
});
it('uses only the grabbed ID when it is outside the selection', () => {
  view({...model,id:'3'}); fireEvent.mouseDown(screen.getByRole('button'), {button:0});
  expect(start).toHaveBeenCalledWith(['3']);
});
