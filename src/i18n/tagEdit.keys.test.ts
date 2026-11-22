import { expect, it } from 'vitest';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
it('provides tag edit labels, errors and matching confirmation placeholders in every language', () => {
  for (const dict of [de, en, es, fr]) {
    for (const key of ['tagMenuRename', 'tagMenuMerge', 'tagRenameName', 'tagMergeTarget', 'tagMergeConfirm', 'tagMergeSubmit', 'tagRenameSubmit', 'errTagNameUnchanged', 'errTagNameExists'] as const) {
      expect(dict[key].trim()).not.toBe('');
      expect(dict[key].match(/\{\w+\}/g)).toEqual(de[key].match(/\{\w+\}/g));
    }
  }
});
