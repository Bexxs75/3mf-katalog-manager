//! JSON form of every `fsops::Error` variant (serde name, the `Display` text, and the shape
//! inside an `Option` and a `Result`), compared byte for byte with a golden file recorded on
//! commit b49dbc1b. The web layer sends these names over the wire, so moving the type in a
//! later step must not change them. Golden files may only be regenerated in step S8.
//!
//! A new variant breaks the build in `index` (exhaustive `match`, no wildcard) and then
//! fails `every_variant_is_listed` until it is added to `ALL`.
use super::*;
use crate::fsops::parity_golden_tests::assert_golden;

const COUNT: usize = 28;

fn index(e: Error) -> usize {
    match e {
        Error::InvalidName => 0,
        Error::InvalidArguments => 1,
        Error::NotFound => 2,
        Error::Conflict => 3,
        Error::Changed => 4,
        Error::Cycle => 5,
        Error::Inconsistent => 6,
        Error::CrossDevice => 7,
        Error::Busy => 8,
        Error::Unsupported => 9,
        Error::TrashForeign => 10,
        Error::TrashMissing => 11,
        Error::TrashConflict => 12,
        Error::PathUnresolvable => 13,
        Error::Protected => 14,
        Error::HasCompanions => 15,
        Error::CompanionsConflict => 16,
        Error::CompanionsChanged => 17,
        Error::CompanionLimit => 18,
        Error::CompanionType => 19,
        Error::CompanionContent => 20,
        Error::CompanionTooLarge => 21,
        Error::UploadTimeout => 22,
        Error::UploadLength => 23,
        Error::InsufficientStorage => 24,
        Error::RestartRequired => 25,
        Error::NotStarted => 26,
        Error::Internal => 27,
    }
}

const ALL: [Error; COUNT] = [
    Error::InvalidName,
    Error::InvalidArguments,
    Error::NotFound,
    Error::Conflict,
    Error::Changed,
    Error::Cycle,
    Error::Inconsistent,
    Error::CrossDevice,
    Error::Busy,
    Error::Unsupported,
    Error::TrashForeign,
    Error::TrashMissing,
    Error::TrashConflict,
    Error::PathUnresolvable,
    Error::Protected,
    Error::HasCompanions,
    Error::CompanionsConflict,
    Error::CompanionsChanged,
    Error::CompanionLimit,
    Error::CompanionType,
    Error::CompanionContent,
    Error::CompanionTooLarge,
    Error::UploadTimeout,
    Error::UploadLength,
    Error::InsufficientStorage,
    Error::RestartRequired,
    Error::NotStarted,
    Error::Internal,
];

#[test]
fn every_variant_is_listed() {
    for (position, e) in ALL.iter().enumerate() {
        assert_eq!(index(*e), position, "{e:?}");
    }
}

#[test]
fn the_json_form_of_every_error_matches_the_golden_file() {
    let mut text = String::new();
    for e in ALL {
        let json = serde_json::to_string(&e).unwrap();
        let back: Error = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
        text += &format!(
            "{e:?} json={json} display={e} some={} err={}\n",
            serde_json::to_string(&Some(e)).unwrap(),
            serde_json::to_string(&Err::<(), Error>(e)).unwrap(),
        );
    }
    assert_golden("error/variants.txt", text.as_bytes());
}
