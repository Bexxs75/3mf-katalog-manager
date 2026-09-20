//! STEP-Datei -> TopoDS_Shape.

use std::path::Path;

use opencascade_sys::{
    if_select::IFSelect_ReturnStatus,
    message::Message_ProgressRange_new,
    step_control::{one_shape_step, read_step, STEPControl_Reader_new},
    topo_ds::TopoDS_Shape,
};

use super::StepError;

/// Liest eine STEP-Datei vollstaendig in eine Shape.
///
/// Bewusst pfadbasiert: OCCT liest die Datei selbst, es wird kein Byte-Puffer
/// durchgereicht.
pub fn read_shape(path: &Path) -> Result<cxx::UniquePtr<TopoDS_Shape>, StepError> {
    let mut reader = STEPControl_Reader_new();
    let status = read_step(reader.pin_mut(), path.to_string_lossy().into_owned());

    if status != IFSelect_ReturnStatus::IFSelect_RetDone {
        return Err(StepError::ReadFailed);
    }

    let progress = Message_ProgressRange_new();
    let roots = reader.pin_mut().TransferRoots(&progress);
    if roots <= 0 {
        return Err(StepError::NoGeometry);
    }

    let shape = one_shape_step(&reader);
    if shape.is_null() {
        return Err(StepError::NoGeometry);
    }

    Ok(shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn read_shape_reads_the_cube_fixture() {
        let shape = read_shape(&fixture("wuerfel-10mm.step")).expect("cube must be readable");
        assert!(!shape.is_null());
    }

    #[test]
    fn read_shape_reads_the_two_body_fixture() {
        let shape = read_shape(&fixture("zwei-koerper.step")).expect("assembly must be readable");
        assert!(!shape.is_null());
    }

    #[test]
    fn read_shape_reports_a_read_failure_for_a_non_step_file() {
        let dir = std::env::temp_dir().join("mf_kat_step_reader_muell");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("kein-step.stp");
        std::fs::write(&path, b"das ist keine STEP-Datei").expect("write");

        let result = read_shape(&path);
        assert!(matches!(
            result,
            Err(StepError::ReadFailed | StepError::NoGeometry)
        ));
    }
}
