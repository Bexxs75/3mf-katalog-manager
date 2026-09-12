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
  displayPreferenceTitle: string;
  displayPreferenceThumbnail: string;
  displayPreferenceRender: string;
  displayPreferenceDescriptionThumbnail: string;
  displayPreferenceDescriptionRender: string;
  rotateLeftAria: string;
  rotateRightAria: string;
  playRotationAria: string;
  pauseRotationAria: string;

  searchPlaceholder: string;
  foldersHeading: string;
  tagsHeading: string;

  previewLabel3d: string;

  columnName: string;
  columnTags: string;
  columnVolume: string;
  columnSize: string;

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
  slicerAutoDetectedLabel: string;

  emptyStateText: string;
  dragToRotate: string;
  metadataHeading: string;
  metaDimensions: string;
  metaVolume: string;
  metaObjectCount: string;
  metaPlateCount: string;
  metaMaterial: string;
  metaFileSize: string;
  metaImported: string;
  noValue: string;
  hashtagsHeading: string;
  addTagPlaceholder: string;

  loadingPreview: string;
  previewUnavailable: string;

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

  backToCatalog: string;
  detailViewer3d: string;
  detailViewerImage: string;
  metaCreator: string;

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

  trashHeading: string;
  bulkSelectedCount: string;
  selectAllLabel: string;
  clearSelectionLabel: string;
  bulkDeleteConfirmQuestion: string;
  trashEmptyState: string;
  emptyTrashButton: string;
  emptyTrashConfirmQuestion: string;
  restoreLabel: string;
  deletePermanentlyLabel: string;
  trashExpiryHint: string;

  collectionsTab: string;
  modelCountLabel: string;
  addToCollectionLabel: string;
  newCollectionPlaceholder: string;
  removeFromCollectionLabel: string;
  deleteCollectionConfirmQuestion: string;
  renameCollectionAria: string;
  noCollectionsEmptyState: string;
  backToCollectionsLabel: string;
  createCollectionLabel: string;
  importFolderAsCollectionOption: string;
}

export function formatCount(forms: PluralForms, n: number): string {
  const form = n === 1 ? forms.one : forms.other;
  return form.replace('{count}', String(n));
}
