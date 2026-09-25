/**
 * Zweite Schranke vor dem Rendern von `sourceUrl` als `<a href>` (das Backend
 * filtert schon beim Schreiben und Lesen): ein `javascript:`-Wert liefe beim
 * Klick im App-Origin mit vollem IPC-Zugriff.
 */
export function isSafeHttpUrl(url: string | null | undefined): url is string {
  if (!url) return false;
  return url.startsWith('http://') || url.startsWith('https://');
}
