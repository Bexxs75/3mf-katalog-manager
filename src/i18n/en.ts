import type { Translations } from './types';

export const en: Translations = {
  import: 'Import',
  importMoreOptionsAria: 'More import options',
  importFilesOption: 'Files...',
  importFolderOption: 'Folder...',

  sortLabel: 'Sort',
  sortName: 'Name',
  sortDate: 'Date',
  sortSize: 'File size',
  sortVolume: 'Volume',

  viewGrid: 'Grid',
  viewList: 'List',

  filesCount: { one: '{count} file', other: '{count} files' },

  settingsTitle: 'Settings',
  appearanceTitle: 'Appearance',
  themeSystem: 'System',
  themeLight: 'Light',
  themeDark: 'Dark',
  themeDescriptionSystem: 'Follows the system setting automatically.',
  themeDescriptionManual: 'Manually set to {mode}.',
  languageTitle: 'Language',

  searchPlaceholder: 'Search name or tag …',
  foldersHeading: 'Folders',
  tagsHeading: 'Tags',
  cloudAccountsHeading: 'Cloud accounts',
  cloudConnected: 'connected',
  cloudError: 'error',
  cloudDisconnected: 'disconnected',
  cloudConnectionError: 'Connection error:',

  previewLabel3d: '3D preview',

  columnOrigin: 'Origin',
  columnName: 'Name',
  columnTags: 'Tags',
  columnVolume: 'Volume',
  columnSize: 'Size',
  columnSync: 'Sync',

  syncSynced: 'Synced',
  syncOutdated: 'Outdated',
  syncLocalOnly: 'Local only',
  syncCloudOnly: 'Cloud only',

  cancel: 'Cancel',
  delete: 'Delete',
  openInSlicer: 'Open in slicer',
  deleteConfirmQuestion: 'Delete entry?',
  deleteAriaLabel: 'Delete entry',

  emptyStateText: 'Select a model to see details, preview, and tags.',
  dragToRotate: 'Drag to rotate',
  metadataHeading: 'Metadata',
  metaDimensions: 'Size',
  metaVolume: 'Volume',
  metaObjectCount: 'Objects',
  metaMaterial: 'Material',
  metaFileSize: 'File size',
  metaImported: 'Imported',
  noValue: '–',
  hashtagsHeading: 'Hashtags',
  addTagPlaceholder: 'Add tag',

  loadingPreview: 'Loading preview …',
  previewUnavailable: 'Preview unavailable',

  cloudBrowserTitle: 'Import from Google Drive',
  cloudBrowserLoading: 'Loading …',
  cloudBrowserEmpty: 'No files in this folder',
  cloudBrowserImportButton: 'Import ({count})',
  importFromCloudOption: 'Import from Google Drive…',
};
