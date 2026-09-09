export type Language = 'de' | 'en' | 'es' | 'fr';

export interface PluralForms {
  one: string;
  other: string;
}

export interface Translations {
  import: string;
  importMoreOptionsAria: string;
  importFilesOption: string;
  importFolderOption: string;

  sortLabel: string;
  sortName: string;
  sortDate: string;
  sortSize: string;
  sortVolume: string;

  viewGrid: string;
  viewList: string;

  filesCount: PluralForms;

  settingsTitle: string;
  appearanceTitle: string;
  themeSystem: string;
  themeLight: string;
  themeDark: string;
  themeDescriptionSystem: string;
  themeDescriptionManual: string;
  languageTitle: string;

  searchPlaceholder: string;
  foldersHeading: string;
  tagsHeading: string;
  cloudAccountsHeading: string;
  cloudConnected: string;
  cloudError: string;
  cloudDisconnected: string;
  cloudConnectionError: string;

  previewLabel3d: string;

  columnOrigin: string;
  columnName: string;
  columnTags: string;
  columnVolume: string;
  columnSize: string;
  columnSync: string;

  syncSynced: string;
  syncOutdated: string;
  syncLocalOnly: string;
  syncCloudOnly: string;

  cancel: string;
  delete: string;
  openInSlicer: string;
  deleteConfirmQuestion: string;
  deleteAriaLabel: string;
  slicerSectionTitle: string;
  noSlicersConfigured: string;
  addSlicer: string;
  confirmSlicerName: string;
  removeSlicerAria: string;
  chooseSlicerAria: string;
  slicerLaunchError: string;

  emptyStateText: string;
  dragToRotate: string;
  metadataHeading: string;
  metaDimensions: string;
  metaVolume: string;
  metaObjectCount: string;
  metaMaterial: string;
  metaFileSize: string;
  metaImported: string;
  noValue: string;
  hashtagsHeading: string;
  addTagPlaceholder: string;

  loadingPreview: string;
  previewUnavailable: string;

  cloudBrowserTitle: string;
  cloudBrowserLoading: string;
  cloudBrowserEmpty: string;
  cloudBrowserImportButton: string;
  cloudBrowserError: string;
  cloudBrowserSelectAll: string;
  cloudBrowserImporting: string;
  importFromCloudOption: string;
}

export function formatCount(forms: PluralForms, n: number): string {
  const form = n === 1 ? forms.one : forms.other;
  return form.replace('{count}', String(n));
}
