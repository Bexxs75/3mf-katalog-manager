import { useCallback, useEffect, useState } from 'react';
import * as updateApi from '../lib/api/update';

export function useUpdateCheck() {
  const [currentVersion, setCurrentVersion] = useState('');
  const [latestVersion, setLatestVersion] = useState('');
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [releaseUrl, setReleaseUrl] = useState('');
  const [dismissed, setDismissed] = useState(false);
  const [checking, setChecking] = useState(false);

  const runCheck = useCallback(() => {
    setChecking(true);
    return updateApi
      .checkForUpdate()
      .then((result) => {
        setCurrentVersion(result.currentVersion);
        setLatestVersion(result.latestVersion);
        setUpdateAvailable(result.updateAvailable);
        setReleaseUrl(result.releaseUrl);
      })
      .catch((e) => {
        // Wie im Backend: ein fehlgeschlagener Update-Check ist niemals ein
        // sichtbarer Fehler, nur ein geloggter Hinweis.
        console.warn('[update-check] Fehlgeschlagen:', e);
      })
      .finally(() => setChecking(false));
  }, []);

  useEffect(() => {
    // Die App-Version ist statisch bekannt und wird unabhaengig vom
    // (netzwerkabhaengigen) Update-Check sofort geladen, damit die Anzeige
    // nicht bis zu 5s auf den Roundtrip warten muss (Final-Review Finding F2).
    updateApi
      .getAppVersion()
      .then((version) => setCurrentVersion(version))
      .catch((e) => {
        console.warn('[update-check] App-Version konnte nicht ermittelt werden:', e);
      });
    runCheck();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const checkNow = useCallback(() => {
    setDismissed(false);
    runCheck();
  }, [runCheck]);

  const dismiss = useCallback(() => setDismissed(true), []);

  const download = useCallback(() => {
    if (!releaseUrl) return;
    updateApi.openReleaseUrl(releaseUrl).catch((e) => {
      console.warn('[update-check] Release-Seite konnte nicht geoeffnet werden:', e);
    });
  }, [releaseUrl]);

  return { currentVersion, latestVersion, updateAvailable, releaseUrl, dismissed, dismiss, checkNow, checking, download };
}
