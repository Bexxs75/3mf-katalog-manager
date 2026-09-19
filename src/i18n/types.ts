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
  viewFolder: string;
  noFolderLabel: string;

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
  foldersInfoTooltip: string;
  allModelsLabel: string;
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
  slicerLaunchError: string;
  slicerAutoDetectedLabel: string;
  slicerPrimaryChip: string;
  setPrimarySlicerAria: string;

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
  railCatalog: string;
  railFilament: string;
  filamentUploadImageLabel: string;
  filamentLocationLabel: string;
  filamentViewDashboard: string;
  filamentViewList: string;
  filamentStatusOk: string;
  filamentStatusLow: string;
  filamentStatusEmpty: string;
  filamentStatTotal: string;
  filamentStatRemaining: string;
  filamentStatLocations: string;
  filamentStatAttention: string;
  filamentSearchPlaceholder: string;
  filamentFilterLow: string;
  filamentFilterEmpty: string;
  filamentOpenAddPanelButton: string;
  filamentImageDropHint: string;
  filamentSectionImage: string;
  filamentSectionIdentification: string;
  filamentSectionStorage: string;
  filamentSectionStock: string;
  filamentNoResults: string;
  filamentColumnStock: string;
  filamentColumnStatus: string;
  filamentQuantityLabel: string;
  filamentQuantityHint: string;

  printedBadge: string;
  notPrintedLabel: string;
  markAsPrinted: string;
  markAsNotPrinted: string;
  metaWeight: string;
  metaWeightFromSlicer: string;
  sliceFilamentHeading: string;
  sliceFilamentPlateLabel: string;
  metaCostEstimate: string;
  costEstimateUnpricedHint: string;
  rescanMetadataButton: string;
  rescanMetadataSuccess: string;
  catalogBackupTitle: string;
  exportCatalogButton: string;
  importCatalogButton: string;
  importCatalogConfirmQuestion: string;
  importCatalogConfirmYes: string;
  importCatalogRestartHint: string;
  catalogSetupTitle: string;
  catalogSetupIntro: string;
  catalogSetupFileTypesNote: string;
  catalogSetupAdoptTitle: string;
  catalogSetupAdoptDescription: string;
  catalogSetupNewTitle: string;
  catalogSetupNewDescription: string;
  catalogSetupLater: string;
  catalogSetupFootnote: string;
  catalogSetupImporting: string;
  catalogSetupSettingUp: string;
  catalogSetupAdoptSummary: string;
  catalogSetupNewSummary: string;
  catalogSetupOpenFolderButton: string;
  catalogSetupDoneButton: string;
  catalogSetupError: string;
  catalogBaseDirSectionTitle: string;
  catalogBaseDirNotSet: string;
  catalogBaseDirChangeButton: string;
  catalogBaseDirSetupButton: string;
  catalogBaseDirOpenButton: string;
  printLogHeading: string;
  printLogAddButton: string;
  printLogDatePlaceholder: string;
  printLogNotePlaceholder: string;
  printLogPhotoButton: string;
  printLogSaveButton: string;
  printLogCancelButton: string;
  printLogEmpty: string;
  printLogDeleteButton: string;

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
  viewAllCollectionsLabel: string;
  modelCountLabel: PluralForms;
  addToCollectionLabel: string;
  newCollectionPlaceholder: string;
  removeFromCollectionLabel: string;
  deleteCollectionConfirmQuestion: string;
  renameCollectionAria: string;
  noCollectionsEmptyState: string;
  createCollectionLabel: string;
  importFolderAsCollectionOption: string;

  createFolderLabel: string;
  newFolderPlaceholder: string;
  settingsTabGeneral: string;
  settingsTabSlicer: string;
  settingsTabCatalog: string;
  settingsTabInfo: string;
  infoAppVersionLabel: string;
  infoUpToDateLabel: string;
  infoUpdateAvailableLabel: string;
  infoCheckForUpdateButton: string;
  infoCheckingForUpdate: string;
  infoViewReleaseNotes: string;
  infoSourceCodeLabel: string;
  infoLicenseLabel: string;
  updateToastText: string;
  updateToastDownloadLabel: string;
}

export function formatCount(forms: PluralForms, n: number): string {
  const form = n === 1 ? forms.one : forms.other;
  return form.replace('{count}', String(n));
}
