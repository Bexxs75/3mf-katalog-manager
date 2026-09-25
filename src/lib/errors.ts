/**
 * Fehlermeldung aus einer abgelehnten Promise. Tauri-Commands lehnen oft mit
 * einem einfachen String ab statt mit einem `Error`-Objekt, deshalb kein
 * `e.message` ohne diese Absicherung.
 */
export function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
