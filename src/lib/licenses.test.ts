import { describe, expect, it } from 'vitest';
import { licenseUsage } from './licenses';

describe('licenseUsage', () => {
  it.each(['cc-by-nc', 'cc-by-nc-sa', 'cc-by-nc-nd', 'personal'])('%s is private', value => {
    expect(licenseUsage(value)).toBe('private');
  });
  it.each(['cc-by', 'cc-by-sa', 'cc-by-nd', 'cc0', 'commercial', 'own'])('%s permits commercial use', value => {
    expect(licenseUsage(value)).toBe('commercial');
  });
  it.each([null, '', 'CC BY', 'my commercial license', ' cc-by ', 'CC-BY'])('%s makes no claim', value => {
    expect(licenseUsage(value)).toBe('unknown');
  });
});
