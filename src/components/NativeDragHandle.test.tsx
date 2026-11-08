import { beforeEach, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LanguageProvider } from '../i18n/LanguageContext';
import { RuntimeEnvironmentProvider } from '../hooks/useRuntimeEnvironment';
import type { ModelFile } from '../types';
const start = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock('../lib/nativeDrag', async importOriginal => ({ ...await importOriginal<typeof import('../lib/nativeDrag')>(), useNativeDragEnabled: () => true, loadNativeDragEnabled: async () => {}, beginDragOut: start }));
import { NativeDragHandle } from './NativeDragHandle';
const model = {id:'1',path:'/catalog/a.stl',name:'a.stl'} as ModelFile;
beforeEach(() => start.mockClear());
function view(file = model, container = false, parent = vi.fn()) {
  return render(<LanguageProvider><RuntimeEnvironmentProvider value={{container}}><div onMouseDown={parent}><NativeDragHandle model={file} selectedIds={new Set(['1','2','step','gcode'])} selectionModels={[model, {...model,id:'2',path:'b.OBJ'}, {...model,id:'step',path:'c.step'}, {...model,id:'gcode',path:'c.gcode'}]} /></div></RuntimeEnvironmentProvider></LanguageProvider>);
}
it('starts the selected IDs once after moving more than four pixels and does not start folder drag', () => {
  const parent = vi.fn(); view(model, false, parent);
  fireEvent.mouseDown(screen.getByRole('button', {hidden:true}), {button:0});
  expect(start).not.toHaveBeenCalled();
  fireEvent.mouseMove(window, {buttons:1, clientX:4});
  expect(start).not.toHaveBeenCalled();
  fireEvent.mouseMove(window, {buttons:1, clientX:5});
  fireEvent.mouseMove(window, {buttons:1, clientX:10});
  expect(start).toHaveBeenCalledTimes(1);
  expect(start).toHaveBeenCalledWith(['1','2']);
  expect(parent).not.toHaveBeenCalled();
});
it('does not start on keyboard click or right mouse button', () => {
  view(); const button = screen.getByRole('button', {hidden:true});
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
  view({...model,id:'3'}); fireEvent.mouseDown(screen.getByRole('button', {hidden:true}), {button:0});
  fireEvent.mouseMove(window, {buttons:1, clientY:5});
  expect(start).toHaveBeenCalledWith(['3']);
});

it('keeps the mouse-only grip out of keyboard and accessibility navigation', () => {
  view();
  const button = screen.getByRole('button', {hidden:true});
  expect(button).toHaveAttribute('tabindex', '-1');
  expect(button).toHaveAttribute('aria-hidden', 'true');
});
it('reports the 64-file limit without invoking native drag', () => {
  const files = Array.from({length:65}, (_, i) => ({...model,id:String(i)}));
  render(<LanguageProvider><RuntimeEnvironmentProvider value={{container:false}}><NativeDragHandle model={model} selectedIds={new Set(files.map(f => f.id))} selectionModels={files} /></RuntimeEnvironmentProvider></LanguageProvider>);
  fireEvent.mouseDown(screen.getByRole('button', {hidden:true}), {button:0});
  expect(start).not.toHaveBeenCalled();
  fireEvent.mouseMove(window, {buttons:1, clientX:5});
  expect(screen.getByRole('alert')).toHaveTextContent('64');
});

it('does not start for a click, released button, blur or unmount', () => {
  const rendered = view(); const button = screen.getByRole('button', {hidden:true});
  fireEvent.mouseDown(button, {button:0});
  fireEvent.mouseUp(window);
  fireEvent.click(button);
  fireEvent.mouseMove(window, {buttons:1, clientX:10});
  expect(start).not.toHaveBeenCalled();
  fireEvent.mouseDown(button, {button:0});
  fireEvent.mouseMove(window, {buttons:0, clientX:10});
  fireEvent.mouseMove(window, {buttons:1, clientX:10});
  expect(start).not.toHaveBeenCalled();
  fireEvent.mouseDown(button, {button:0});
  fireEvent.blur(window);
  fireEvent.mouseMove(window, {buttons:1, clientX:10});
  expect(start).not.toHaveBeenCalled();
  fireEvent.mouseDown(button, {button:0});
  rendered.unmount();
  fireEvent.mouseMove(window, {buttons:1, clientX:10});
  expect(start).not.toHaveBeenCalled();
});

it('keeps the drag target at least 24 by 24 pixels even in a shrinking list row', () => {
  view();
  expect(screen.getByRole('button', { hidden: true })).toHaveClass('min-w-6', 'min-h-6', 'shrink-0');
});

it('uses the external icon and the localized explanation for dragging out', async () => {
  const { de } = await import('../i18n/de');
  const { icons } = await import('./icons.generated');
  view();
  const button = screen.getByRole('button', { hidden: true });
  expect(button).toHaveAttribute('title', de.dgHandle);
  expect(button.querySelector('svg')).toContainHTML(icons.external);
  expect(button).not.toHaveTextContent('⠿');
});
