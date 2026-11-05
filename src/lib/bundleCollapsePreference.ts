const STORAGE_KEY = '3mf-katalog-bundle-collapse';

/** Whether the catalog shows a bundle as one card; on unless the user turned it off, also without readable storage. */
export function loadBundleCollapse(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) !== 'off';
  } catch {
    return true;
  }
}

export function saveBundleCollapse(collapse: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, collapse ? 'on' : 'off');
  } catch {
    // Storage blocked: the choice only applies to this session.
  }
}
