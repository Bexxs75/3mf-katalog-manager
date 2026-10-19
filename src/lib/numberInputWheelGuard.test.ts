import { afterEach, beforeEach, expect, it } from 'vitest';
import { fireEvent } from '@testing-library/react';
import { installNumberInputWheelGuard } from './numberInputWheelGuard';

let uninstall: () => void;
beforeEach(() => { uninstall = installNumberInputWheelGuard(document); });
afterEach(() => { uninstall(); document.body.innerHTML = ''; });

function addInput(type: string): HTMLInputElement {
  const input = document.createElement('input');
  input.type = type;
  input.value = type === 'number' ? '5' : 'text';
  document.body.appendChild(input);
  return input;
}

it('takes focus from a focused number field when the wheel turns over it', () => {
  const input = addInput('number');
  input.focus();
  fireEvent.wheel(input, { deltaY: 100 });
  expect(document.activeElement).not.toBe(input);
  expect(input.value).toBe('5');
});

it('does not cancel the wheel event, so the page keeps scrolling', () => {
  const input = addInput('number');
  input.focus();
  expect(fireEvent.wheel(input, { deltaY: 100, cancelable: true })).toBe(true);
});

it('leaves arrow keys and a click-focused number field alone without wheel', () => {
  const input = addInput('number');
  input.focus();
  fireEvent.keyDown(input, { key: 'ArrowUp' });
  expect(document.activeElement).toBe(input);
});

it('keeps focus in other field types and in unfocused number fields', () => {
  const text = addInput('text');
  text.focus();
  fireEvent.wheel(text, { deltaY: 100 });
  expect(document.activeElement).toBe(text);

  const number = addInput('number');
  fireEvent.wheel(number, { deltaY: 100 });
  expect(document.activeElement).toBe(text);
});

it('stops acting once uninstalled', () => {
  uninstall();
  const input = addInput('number');
  input.focus();
  fireEvent.wheel(input, { deltaY: 100 });
  expect(document.activeElement).toBe(input);
  uninstall = installNumberInputWheelGuard(document);
});
