import { describe, expect, it } from 'vitest';
import { isFromInteractiveElement } from './spoolCardEvents';

function card() {
  const root = document.createElement('div');
  root.innerHTML = '<span class="text">PLA</span><button>✎</button><input />';
  return root;
}

describe('isFromInteractiveElement', () => {
  it('accepts plain card content', () => {
    const root = card();
    expect(isFromInteractiveElement({ target: root.querySelector('.text'), currentTarget: root })).toBe(false);
    expect(isFromInteractiveElement({ target: root, currentTarget: root })).toBe(false);
  });

  it('ignores buttons and inputs inside the card', () => {
    const root = card();
    expect(isFromInteractiveElement({ target: root.querySelector('button'), currentTarget: root })).toBe(true);
    expect(isFromInteractiveElement({ target: root.querySelector('input'), currentTarget: root })).toBe(true);
  });

  it('ignores events that bubbled in from a portal outside the card DOM', () => {
    const root = card();
    const outside = document.createElement('div');
    expect(isFromInteractiveElement({ target: outside, currentTarget: root })).toBe(true);
  });
});
