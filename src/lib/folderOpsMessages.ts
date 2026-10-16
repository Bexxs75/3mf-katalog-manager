import type { Language } from '../i18n/types';
const messages = {
  de: { create: 'Unterordner anlegen', rename: 'Ordner umbenennen', move: 'Ordner verschieben', target: 'Zielordner', name: 'Ordnername', confirm: 'Bestätigen', contents: 'Alle Inhalte, auch nicht katalogisierte Dateien, werden auf dem Server mitverschoben.', moveFile: 'Datei auf dem Server verschieben?', moveFolder: 'Ordner auf dem Server verschieben?', failed: 'Die Ordneraktion ist fehlgeschlagen.', loading: 'Zusammenfassung wird geladen …' },
  en: { create: 'Create subfolder', rename: 'Rename folder', move: 'Move folder', target: 'Destination folder', name: 'Folder name', confirm: 'Confirm', contents: 'All contents, including uncatalogued files, will move on the server.', moveFile: 'Move file on the server?', moveFolder: 'Move folder on the server?', failed: 'The folder operation failed.', loading: 'Loading summary …' },
  es: { create: 'Crear subcarpeta', rename: 'Renombrar carpeta', move: 'Mover carpeta', target: 'Carpeta de destino', name: 'Nombre de carpeta', confirm: 'Confirmar', contents: 'Se moverá todo el contenido en el servidor, incluidos los archivos no catalogados.', moveFile: '¿Mover el archivo en el servidor?', moveFolder: '¿Mover la carpeta en el servidor?', failed: 'La operación de carpeta ha fallado.', loading: 'Cargando resumen …' },
  fr: { create: 'Créer un sous-dossier', rename: 'Renommer le dossier', move: 'Déplacer le dossier', target: 'Dossier de destination', name: 'Nom du dossier', confirm: 'Confirmer', contents: 'Tout le contenu sera déplacé sur le serveur, y compris les fichiers non catalogués.', moveFile: 'Déplacer le fichier sur le serveur ?', moveFolder: 'Déplacer le dossier sur le serveur ?', failed: 'L’opération sur le dossier a échoué.', loading: 'Chargement du résumé …' },
};
export function folderOpsMessages(language?: Language) {
  const key = language ?? localStorage.getItem('3mf-katalog-language');
  return messages[key && key in messages ? key as Language : 'de'];
}
