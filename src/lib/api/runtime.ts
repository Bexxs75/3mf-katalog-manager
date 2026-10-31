import { invoke } from '../transport';

export interface RuntimeEnvironment { container: boolean; web?: boolean; readOnly?: boolean }

let environment: Promise<RuntimeEnvironment> | undefined;

// Share the startup request across StrictMode mounts and all consumers.
export function getRuntimeEnvironment(): Promise<RuntimeEnvironment> {
  return environment ??= invoke<RuntimeEnvironment>('get_runtime_environment')
    .catch(() => ({ container: false }));
}

export const hasNetworkFilesystemWarning = () => invoke<boolean>('has_network_filesystem_warning');

export interface CatalogWebLink { url: string | null; path: string }

export const getCatalogWebLink = (fileId?: string) =>
  invoke<CatalogWebLink>('get_catalog_web_link', { fileId: fileId === undefined ? null : Number(fileId) });
