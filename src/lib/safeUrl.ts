/**
 * Zweite Schranke vor dem Rendern eines aus der Datenbank gelesenen
 * `sourceUrl`-Werts als `<a href>`. Das Backend filtert bereits beim
 * Schreiben (`validate_source_url`) und beim Lesen (`sanitize_source_url`);
 * diese Pruefung haelt die Annahme "href ist immer http(s)" auch dann
 * aufrecht, wenn ein Wert auf einem anderen Weg ins Frontend gelangt -
 * ein `javascript:`-Wert wuerde beim Klick im App-Origin ausgefuehrt und
 * haette dort vollen IPC-Zugriff (Security-Review 2026-09-19, Finding I-2).
 */
export function isSafeHttpUrl(url: string | null | undefined): url is string {
  if (!url) return false;
  return url.startsWith('http://') || url.startsWith('https://');
}
