//! Bild per Drag & Drop in das Spulenformular.
//!
//! Unter `dragDropEnabled` kommen Datei-Drops nur als Tauri-Ereignis an; das
//! Frontend kennt dann den Pfad und fragt `read_dropped_image` an. Damit daraus
//! kein beliebiges Datei-Lesen wird (gleiches Muster wie `import_dropped` +
//! `PendingArchives`): Das Backend merkt sich die Bildpfade selbst aus dem
//! Fenster-Ereignis `DragDrop::Drop` (siehe `lib.rs`) und liest nur diese,
//! jeden hoechstens einmal, mit derselben Groessen- und Formatpruefung wie der
//! Klick-Upload (`pick_and_read_image`).

use super::*;

use super::files::{read_image_bounded, MAX_CUSTOM_IMAGE_BYTES};

/// Wie der Dateidialog-Filter von `pick_and_read_image`.
const DROPPABLE_IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

pub(crate) fn is_droppable_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| DROPPABLE_IMAGE_EXTENSIONS.contains(&e.as_str()))
}

/// Bildpfade des letzten vom Backend beobachteten Drops.
#[derive(Default)]
pub struct DroppedImages {
    observed: Mutex<HashSet<PathBuf>>,
}

impl DroppedImages {
    /// Vom Fenster-Ereignis `DragDrop::Drop` aufgerufen; ersetzt den vorigen Drop.
    pub(crate) fn observe_drop(&self, paths: &[PathBuf]) {
        if let Ok(mut set) = self.observed.lock() {
            set.clear();
            set.extend(paths.iter().filter(|p| p.is_file() && is_droppable_image_path(p)).cloned());
        }
    }

    /// Verbraucht die Freigabe fuer `path`.
    pub(crate) fn claim(&self, path: &Path) -> bool {
        self.observed.lock().map(|mut set| set.remove(path)).unwrap_or(false)
    }
}

pub(crate) fn read_dropped_image_with(images: &DroppedImages, path: &str) -> CmdResult<String> {
    use base64::Engine;
    let path = PathBuf::from(path);
    if !is_droppable_image_path(&path) {
        return Err("Nur PNG-, JPG- oder WebP-Bilder werden unterstuetzt".to_string());
    }
    if !images.claim(&path) {
        return Err("Bild wurde nicht per Drag & Drop uebergeben".to_string());
    }
    let bytes = read_image_bounded(&path, MAX_CUSTOM_IMAGE_BYTES as u64)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
pub async fn read_dropped_image(app: tauri::AppHandle, path: String) -> CmdResult<String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let images = app.state::<DroppedImages>();
        read_dropped_image_with(&images, &path)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(name: &str, bytes: usize) -> PathBuf {
        let path = crate::commands::unique_test_dir("dropped_image").join(name);
        std::fs::write(&path, vec![7u8; bytes]).expect("write");
        path
    }

    #[test]
    fn is_droppable_image_path_matches_the_click_upload_filter() {
        for ok in ["a.png", "a.JPG", "a.jpeg", "b/c.WebP"] {
            assert!(is_droppable_image_path(Path::new(ok)), "{ok}");
        }
        for bad in ["a.gif", "a.txt", "a", ".png", "a.png.exe"] {
            assert!(!is_droppable_image_path(Path::new(bad)), "{bad}");
        }
    }

    #[test]
    fn an_observed_image_can_be_read_exactly_once() {
        let images = DroppedImages::default();
        let png = write_file("spule.PNG", 3);
        images.observe_drop(std::slice::from_ref(&png));

        let first = read_dropped_image_with(&images, png.to_str().unwrap()).expect("read");

        assert_eq!(first, "BwcH", "base64 von [7, 7, 7]");
        assert!(read_dropped_image_with(&images, png.to_str().unwrap()).is_err(), "Freigabe gilt nur einmal");
    }

    #[test]
    fn an_image_that_was_not_dropped_is_refused() {
        let images = DroppedImages::default();
        let png = write_file("fremd.png", 3);
        assert!(read_dropped_image_with(&images, png.to_str().unwrap()).is_err());
    }

    #[test]
    fn a_new_drop_replaces_the_previous_one() {
        let images = DroppedImages::default();
        let first = write_file("eins.png", 3);
        let second = write_file("zwei.png", 3);
        images.observe_drop(std::slice::from_ref(&first));
        images.observe_drop(std::slice::from_ref(&second));

        assert!(read_dropped_image_with(&images, first.to_str().unwrap()).is_err());
        assert!(read_dropped_image_with(&images, second.to_str().unwrap()).is_ok());
    }

    #[test]
    fn non_image_files_are_neither_observed_nor_read() {
        let images = DroppedImages::default();
        let txt = write_file("notiz.txt", 3);
        images.observe_drop(std::slice::from_ref(&txt));
        assert!(read_dropped_image_with(&images, txt.to_str().unwrap()).is_err());
    }

    #[test]
    fn an_oversized_image_is_refused() {
        let images = DroppedImages::default();
        let big = write_file("gross.png", 6 * 1024 * 1024);
        images.observe_drop(std::slice::from_ref(&big));
        assert!(read_dropped_image_with(&images, big.to_str().unwrap()).is_err());
    }
}
