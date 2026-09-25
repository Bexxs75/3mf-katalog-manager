import type { Language } from './types';

const LOCALE_MAP: Record<Language, string> = {
  de: 'de-DE',
  en: 'en-US',
  es: 'es-ES',
  fr: 'fr-FR',
};

function localeFor(language: Language): string {
  return LOCALE_MAP[language];
}

const BYTE_UNITS = ['B', 'KB', 'MB', 'GB'] as const;

export function formatBytes(bytes: number, language: Language): string {
  const locale = localeFor(language);
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < BYTE_UNITS.length - 1) {
    value /= 1024;
    unitIndex++;
  }
  const decimals = unitIndex < 2 ? 0 : 1;
  const formatted = new Intl.NumberFormat(locale, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  }).format(value);
  return `${formatted} ${BYTE_UNITS[unitIndex]}`;
}

export function formatVolumeCm3(volumeCm3: number | null, language: Language): string {
  if (volumeCm3 === null) return '–';
  const formatted = new Intl.NumberFormat(localeFor(language), {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  }).format(volumeCm3);
  return `${formatted} cm³`;
}

export function formatDimensions(
  dimensionsMm: [number, number, number] | null,
  language: Language,
): string {
  if (dimensionsMm === null) return '–';
  const nf = new Intl.NumberFormat(localeFor(language), { maximumFractionDigits: 0 });
  const [x, y, z] = dimensionsMm;
  return `${nf.format(x)} × ${nf.format(y)} × ${nf.format(z)} mm`;
}

export function formatWeightG(grams: number, language: Language): string {
  return `${new Intl.NumberFormat(localeFor(language)).format(grams)} g`;
}

/** Restgewicht einer Spule: immer eine Nachkommastelle (Zehntelgramm). */
export function formatStockG(grams: number, language: Language): string {
  const nf = new Intl.NumberFormat(localeFor(language), { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  return `${nf.format(grams)} g`;
}

export function formatLengthM(meters: number, language: Language): string {
  const nf = new Intl.NumberFormat(localeFor(language), { maximumFractionDigits: 1 });
  return `${nf.format(meters)} m`;
}

/** Verbrauchte Filamentlaenge eines Druckauftrags in mm (gerundet). */
export function formatLengthMm(mm: number, language: Language): string {
  return `${new Intl.NumberFormat(localeFor(language)).format(Math.round(mm))} mm`;
}

/**
 * Nur die Minutenzahl (mind. 1), lokalisiert - die Einheit kommt aus dem
 * Uebersetzungstext `printerJobsMinutes` (bewusst kein hartkodiertes " min").
 */
export function formatDurationMinutes(seconds: number, language: Language): string {
  const minutes = Math.max(1, Math.round(seconds / 60));
  return new Intl.NumberFormat(localeFor(language)).format(minutes);
}

export function formatDiameterMm(diameterMm: number, language: Language): string {
  const formatted = new Intl.NumberFormat(localeFor(language), {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(diameterMm);
  return `${formatted} mm`;
}

export function formatPrice(price: number, language: Language): string {
  return new Intl.NumberFormat(localeFor(language), {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(price);
}

export function formatDate(rfc3339: string, language: Language): string {
  const date = new Date(rfc3339);
  if (Number.isNaN(date.getTime())) return '–';
  return new Intl.DateTimeFormat(localeFor(language)).format(date);
}

/** Datum + Uhrzeit aus Unix-Sekunden (z. B. `PrinterConnection.connectedSince`). */
export function formatDateTime(unixSeconds: number, language: Language): string {
  const date = new Date(unixSeconds * 1000);
  if (Number.isNaN(date.getTime())) return '–';
  return new Intl.DateTimeFormat(localeFor(language), { dateStyle: 'medium', timeStyle: 'short' }).format(date);
}

/** Nur die Uhrzeit aus Unix-Sekunden (z. B. Zeitpunkt eines Verbindungsfehlers). */
export function formatTime(unixSeconds: number, language: Language): string {
  const date = new Date(unixSeconds * 1000);
  if (Number.isNaN(date.getTime())) return '–';
  return new Intl.DateTimeFormat(localeFor(language), { timeStyle: 'short' }).format(date);
}

export function formatRelativeTime(rfc3339: string, language: Language): string {
  const date = new Date(rfc3339);
  if (Number.isNaN(date.getTime())) return '–';

  const diffSeconds = Math.round((date.getTime() - Date.now()) / 1000);
  const absSeconds = Math.abs(diffSeconds);
  const rtf = new Intl.RelativeTimeFormat(localeFor(language), { numeric: 'auto' });

  if (absSeconds < 60) return rtf.format(0, 'second');
  if (absSeconds < 3600) return rtf.format(Math.round(diffSeconds / 60), 'minute');
  if (absSeconds < 86400) return rtf.format(Math.round(diffSeconds / 3600), 'hour');
  return rtf.format(Math.round(diffSeconds / 86400), 'day');
}
