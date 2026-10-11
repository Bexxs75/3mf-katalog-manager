import presets from './licensePresets.json';

type LicenseLabelKey = 'licenseCcBy' | 'licenseCcBySa' | 'licenseCcByNc' | 'licenseCcByNcSa' | 'licenseCcByNd' | 'licenseCcByNcNd' | 'licenseCc0' | 'licensePersonal' | 'licenseCommercial' | 'licenseOwn';
export type LicenseUsage = 'private' | 'commercial' | 'unknown';
export const licensePresets = presets as { code: string; usage: Exclude<LicenseUsage, 'unknown'>; label: LicenseLabelKey }[];

export function licenseUsage(license: string | null | undefined): LicenseUsage {
  return licensePresets.find(preset => preset.code === license)?.usage ?? 'unknown';
}

export function licenseLabel(license: string | null | undefined, t: (key: LicenseLabelKey | 'licenseUnset') => string): string {
  const preset = licensePresets.find(preset => preset.code === license);
  return preset ? t(preset.label) : license || t('licenseUnset');
}

export const usageLabels = {
  private: 'licenseUsagePrivate', commercial: 'licenseUsageCommercial', unknown: 'licenseUsageUnknown',
} as const;

export function modelLicenseAriaLabel(
  name: string,
  license: string | null | undefined,
  t: (key: typeof usageLabels[LicenseUsage]) => string,
): string {
  const usage = licenseUsage(license);
  return usage === 'unknown' ? name : `${name}, ${t(usageLabels[usage])}`;
}
