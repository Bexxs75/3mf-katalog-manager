import { describe, expect, it, vi } from 'vitest';
import { contextSelection, modelMenuActions, runModelSelection, selectionState } from './modelMenuActions';
import { makeModelFile } from '../test/factories';
import { de } from '../i18n/de';
import { en } from '../i18n/en';

describe('model menu definitions', () => {
  it.each([false, true])('has translated labels and unique shortcuts (web=%s)', web => {
    for (const count of [1, 3]) {
      const actions = modelMenuActions({ web, count, favorite: 'mixed', queue: false, printed: true });
      const shortcuts = actions.flatMap(action => action.shortcut ?? []);
      expect(new Set(shortcuts).size).toBe(shortcuts.length);
      expect(shortcuts).toEqual(count === 1 ? ['F2', 'F6', 'Delete', 'Shift+Delete'] : ['F6', 'Delete', 'Shift+Delete']);
      for (const action of actions) {
        expect(de[action.label]).toBeTruthy(); expect(en[action.label]).toBeTruthy();
        if (action.shortcutLabel) { expect(de[action.shortcutLabel]).toBeTruthy(); expect(en[action.shortcutLabel]).toBeTruthy(); }
      }
      expect(actions.some(action => action.id === 'download')).toBe(web);
      expect(actions.some(action => action.id === 'slicer')).toBe(!web);
    }
  });
  it('uses all selected models only if the context model belongs to the selection', () => {
    const models = ['a', 'b', 'c'].map(id => makeModelFile({ id }));
    expect(contextSelection(models, 'b', new Set(['a', 'b'])).map(model => model.id)).toEqual(['a', 'b']);
    expect(contextSelection(models, 'c', new Set(['a', 'b'])).map(model => model.id)).toEqual(['c']);
  });
  it('visits every model even when one action fails', async () => {
    const action = vi.fn(async (id: string) => { if (id === 'a') throw new Error('failure'); });
    await expect(runModelSelection(['a', 'b', 'c'], action)).rejects.toThrow('failure');
    expect(action.mock.calls).toEqual([['a'], ['b'], ['c']]);
  });
  it('distinguishes all, none and mixed states', () => {
    expect(selectionState([true, true])).toBe(true);
    expect(selectionState([false, false])).toBe(false);
    expect(selectionState([true, false])).toBe('mixed');
  });
});

it('keeps toolbar extensions translated and preserves shared action labels and shortcuts', () => {
  const options = { web: false, count: 3, favorite: true, queue: true, printed: true };
  const context = modelMenuActions(options);
  const toolbar = modelMenuActions({ ...options, toolbar: true });
  for (const action of toolbar) {
    expect(de[action.label]).toBeTruthy();
    expect(en[action.label]).toBeTruthy();
    const shared = context.find(entry => entry.id === action.id);
    if (shared) {
      expect(action.label).toBe(shared.label);
      expect(action.shortcut).toBe(shared.shortcut);
    }
  }
  const shortcuts = toolbar.flatMap(action => action.shortcut ?? []);
  expect(new Set(shortcuts).size).toBe(shortcuts.length);
});

it('has matching nonempty WebFileActions keys in DE and EN', () => {
  const keys = Object.keys(de).filter(key => key.startsWith('webFile')) as (keyof typeof de)[];
  expect(keys.length).toBe(25);
  for (const key of keys) { expect(de[key]).toBeTruthy(); expect(en[key]).toBeTruthy(); }
});
