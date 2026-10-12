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
  it.each(['button', 'menuitem', 'menuitemradio', 'menuitemcheckbox', 'option', 'tab', 'switch', 'checkbox', 'radio', 'combobox', 'slider', 'separator', 'tree', 'treeitem', 'listbox', 'spinbutton', 'grid', 'scrollbar', 'textbox', 'searchbox'])('blocks target role %s and descendants', role => {
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

const detailArrows = { allowDetailNavigationArrows: true };
const allowedDetailTargets = [
  '<div data-detail-viewer tabindex="0"><span></span></div>',
  '<div data-detail-viewer><canvas tabindex="0"></canvas></div>',
  '<a href="#"><span></span></a>',
  '<button><span></span></button>',
  '<div role="group"><button><span></span></button></div>',
  '<div data-detail-viewer><button data-detail-view-toggle><span></span></button></div>',
];
const blockedDetailTargets = [
  '<input />', '<textarea></textarea>', '<select></select>', '<summary></summary>',
  '<div contenteditable="true"><span></span></div>',
  ...['button', 'slider', 'combobox', 'listbox', 'menu', 'menubar', 'menuitem', 'menuitemradio',
    'menuitemcheckbox', 'tab', 'tablist', 'radio', 'radiogroup', 'checkbox', 'switch', 'spinbutton',
    'tree', 'treeitem', 'treegrid', 'option', 'separator', 'toolbar', 'grid', 'scrollbar', 'textbox', 'searchbox']
    .map(role => `<div role="${role}"><button><span></span></button></div>`),
  '<button role="tab"><span></span></button>',
  '<a href="#" role="slider"><span></span></a>',
  '<div contenteditable=""><button><span></span></button></div>',
  '<div role="toolbar"><button data-detail-view-toggle><span></span></button></div>',
  '<button role="button"><span></span></button>',
  '<button contenteditable="true"><span></span></button>',
  '<a href="#" contenteditable="true"><span></span></a>',
  '<div data-plate-selector><button><span></span></button></div>',
];

describe('detail arrow exception', () => {
  it.each(allowedDetailTargets)('allows focused and targeted %s only for detail arrows', markup => {
    document.body.innerHTML = markup;
    const target = document.body.querySelector('span, canvas, button, a, [tabindex]')! as HTMLElement;
    target.tabIndex = 0;
    target.focus();
    for (const key of ['ArrowLeft', 'ArrowRight']) {
      const event = new KeyboardEvent('keydown', { key, bubbles: true });
      target.dispatchEvent(event);
      expect(shouldIgnoreCatalogShortcut(event, detailArrows)).toBe(false);
      expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { key }), detailArrows)).toBe(false);
      expect(shouldIgnoreCatalogShortcut(event)).toBe(true);
    }
    for (const key of ['Enter', ' ', 'Home', 'End', 'ArrowUp', 'ArrowDown', 'a']) {
      expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { key }), detailArrows)).toBe(true);
    }
  });
  it.each(blockedDetailTargets)('keeps widget ownership for %s', markup => {
    document.body.innerHTML = `<div data-detail-viewer>${markup}</div>`;
    const root = document.body.firstElementChild!.firstElementChild!;
    const target = (root.querySelector('span') ?? root) as HTMLElement;
    target.tabIndex = 0;
    for (const key of ['ArrowLeft', 'ArrowRight']) {
      const event = new KeyboardEvent('keydown', { key, bubbles: true });
      target.dispatchEvent(event);
      expect(shouldIgnoreCatalogShortcut(event, detailArrows)).toBe(true);
      target.focus();
      expect(shouldIgnoreCatalogShortcut(new KeyboardEvent('keydown', { key }), detailArrows)).toBe(true);
      target.blur();
    }
  });

});
