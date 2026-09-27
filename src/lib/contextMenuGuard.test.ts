import { afterEach, describe, expect, it } from 'vitest';
import { installContextMenuGuard } from './contextMenuGuard';

function rightClick(target: Element): boolean {
  const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

describe('installContextMenuGuard', () => {
  let uninstall: (() => void) | undefined;
  afterEach(() => {
    uninstall?.();
    document.body.innerHTML = '';
    window.getSelection()?.removeAllRanges();
  });

  it('suppresses the built-in menu on normal content', () => {
    uninstall = installContextMenuGuard(document, false);
    const div = document.body.appendChild(document.createElement('div'));
    expect(rightClick(div)).toBe(true);
  });

  it('keeps the menu in text fields so copy and paste still work', () => {
    uninstall = installContextMenuGuard(document, false);
    const input = document.body.appendChild(document.createElement('input'));
    const textarea = document.body.appendChild(document.createElement('textarea'));
    const editable = document.body.appendChild(document.createElement('div'));
    editable.setAttribute('contenteditable', 'true');
    const inner = editable.appendChild(document.createElement('span'));
    expect(rightClick(input)).toBe(false);
    expect(rightClick(textarea)).toBe(false);
    expect(rightClick(inner)).toBe(false);
  });

  it('keeps the menu when text is selected', () => {
    uninstall = installContextMenuGuard(document, false);
    const p = document.body.appendChild(document.createElement('p'));
    p.textContent = 'Ein Modellname';
    const range = document.createRange();
    range.selectNodeContents(p);
    window.getSelection()?.addRange(range);
    expect(rightClick(p)).toBe(false);
  });

  it('does nothing in development builds', () => {
    uninstall = installContextMenuGuard(document, true);
    const div = document.body.appendChild(document.createElement('div'));
    expect(rightClick(div)).toBe(false);
  });

  it('can be removed again', () => {
    installContextMenuGuard(document, false)();
    const div = document.body.appendChild(document.createElement('div'));
    expect(rightClick(div)).toBe(false);
  });
});
