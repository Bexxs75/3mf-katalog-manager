import { afterEach, describe, expect, it } from 'vitest';
import { shouldIgnoreCatalogShortcut } from './keyboardGuard';
afterEach(() => { document.body.innerHTML = ''; });
describe('catalog keyboard guard', () => {
  it.each(['altKey', 'ctrlKey', 'metaKey', 'shiftKey'])('blocks %s on navigation', modifier => {
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { key: 'ArrowRight', [modifier]: true }))).toBe(true);
  });
  it('blocks IME and consumed events', () => {
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { isComposing: true }))).toBe(true);
    const event = new KeyboardEvent('keydown', { cancelable: true }); event.preventDefault();
    expect(shouldIgnoreCatalogShortcut(event)).toBe(true);
  });
  it.each(['button', 'input', 'select', 'textarea', 'summary', 'a'])('blocks focused %s', tag => {
    const el = document.createElement(tag); el.tabIndex = 0; if (tag === 'a') el.setAttribute('href', '#'); document.body.append(el); el.focus();
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown'))).toBe(true);
  });
  it.each(['button', 'menuitem', 'menuitemradio', 'menuitemcheckbox', 'option', 'tab', 'switch', 'checkbox', 'radio', 'combobox', 'slider', 'separator', 'tree', 'treeitem', 'listbox', 'spinbutton'])('blocks target role %s and descendants', role => {
    const el = document.createElement('div'); el.setAttribute('role', role); const child = document.createElement('span'); el.append(child); document.body.append(el);
    const event = new KeyboardEvent('keydown', { bubbles: true }); child.addEventListener('keydown', e => expect(shouldIgnoreCatalogShortcut(e)).toBe(true)); child.dispatchEvent(event);
  });
  it.each(['[data-navigation-menu]', '[role="menu"]', '[role="listbox"]', '[role="dialog"]', '.menu'])('blocks open %s', selector => {
    const el = document.createElement('div'); if (selector === '.menu') el.className = 'menu'; else if (selector.includes('role')) el.setAttribute('role', selector.split('"')[1]); else el.setAttribute('data-navigation-menu', ''); document.body.append(el);
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown'))).toBe(true);
  });
  it('keeps modal protection even for explicit exceptions', () => {
    document.body.innerHTML = '<div role="dialog" aria-modal="true"></div>';
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown'), { allowModifiers: true, allowInteractive: true, allowMenus: true })).toBe(true);
  });
  it('allows catalog tiles and shifted punctuation', () => {
    const tile = document.createElement('div'); tile.setAttribute('data-model-id', 'a'); tile.tabIndex = 0; document.body.append(tile); tile.focus();
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { key: 'ArrowRight' }))).toBe(false);
    expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { key: '?', shiftKey: true }))).toBe(false);
  });
});
