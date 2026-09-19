import { invoke } from '@tauri-apps/api/core';
import type { CatalogIssues } from '../../types';

export function scanCatalogIssues() {
  return invoke<CatalogIssues>('scan_catalog_issues');
}
