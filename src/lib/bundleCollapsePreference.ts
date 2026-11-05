const STORAGE_KEY = '3mf-katalog-bundle-collapse';

/** Whether the catalog shows a bundle as one card; off unless the user turned it on, also without readable storage. */
export function loadBundleCollapse(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'on';
  } catch {
    return false;
  }
}

export function saveBundleCollapse(collapse: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, collapse ? 'on' : 'off');
  } catch {
    // Storage blocked: the choice only applies to this session.
  }
}
