export type Language = 'de' | 'en' | 'es' | 'fr';

export interface PluralForms {
  one: string;
  other: string;
}

export interface Translations {
  import: string;
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
  densityTitle: string;
  densityCompact: string;
  densityComfort: string;
  densityDescriptionCompact: string;
  densityDescriptionComfort: string;

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
  uploadToCloudAria: string;
  uploadingToCloudAria: string;
  alreadyInCloudAria: string;
  connectCloudToUploadAria: string;
  cloudUploadError: string;
  uploadButtonLabel: string;
  uploadButtonLabelInProgress: string;
  uploadButtonLabelDone: string;

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

  importFromCloudOption: string;
  importCloudFolderOption: string;

  sortLastViewed: string;
  newBadge: string;

  filamentCatalogAria: string;
  filamentDialogTitle: string;
  filamentEmptyState: string;
  filamentMaterialLabel: string;
  filamentManufacturerLabel: string;
  filamentColorLabel: string;
  filamentDiameterLabel: string;
  filamentOriginalWeightLabel: string;
  filamentRemainingWeightLabel: string;
  filamentPriceLabel: string;
  filamentAddButton: string;
  filamentSaveButton: string;
  filamentEditAria: string;
  filamentError: string;
  filamentNavButton: string;
  filamentBackToCatalogButton: string;
  filamentUploadImageLabel: string;

  printedBadge: string;
  notPrintedLabel: string;
  markAsPrinted: string;
  markAsNotPrinted: string;
  metaWeight: string;

  creatorsHeading: string;

  importSummaryText: string;

  uploadModelImageLabel: string;

  metaSourceUrl: string;
  sourceUrlPlaceholder: string;

  queueHeading: string;
  queueEmptyState: string;
  inQueueLabel: string;
  notInQueueLabel: string;
  addToQueue: string;
  removeFromQueue: string;
  savedFiltersHeading: string;
  savedFilterNamePlaceholder: string;

  catalogCleanupTitle: string;
  catalogCleanupScanButton: string;
  catalogCleanupScanning: string;
  catalogCleanupError: string;
  cleanupDialogTitle: string;
  cleanupNoIssues: string;
  cleanupOrphanedHeading: string;
  cleanupDuplicateGroupHeading: string;
  cleanupKeepOldest: string;
  cleanupDeleteSelected: string;

  favoriteAdd: string;
  favoriteRemove: string;
}

export function formatCount(forms: PluralForms, n: number): string {
  const form = n === 1 ? forms.one : forms.other;
  return form.replace('{count}', String(n));
}
