//! Which released app version last understood a given catalog schema. After an
//! update or a manual install the startup notice uses it to name the version to
//! go back to when the update copy does not say so.
/// `(schema version, newest release that works with exactly that schema)`.
/// Releases that share a schema are listed by their newest member, because that
/// is the one worth installing again. Schemas that only ever existed in
/// development builds have no row: no release can read them. The row for the
/// schema of this build names the version being developed; a test fails when the
/// schema grows without a row being added here.
pub const SCHEMA_APP_VERSIONS: &[(i64, &str)] = &[
    (23, "0.12.1"),
    (31, "0.13.0"),
    (32, "0.13.1"),
    (37, "0.14.0"),
    (39, "0.15.4"),
    (47, "0.16.3"),
    (52, "0.17.1"),
    (62, "0.18.0"),
];

/// The release to reinstall for a catalog that is at `schema`, if one is known.
pub fn app_version_for_schema(schema: i64) -> Option<&'static str> {
    SCHEMA_APP_VERSIONS.iter().find(|(s, _)| *s == schema).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations_current_version;

    #[test]
    fn every_schema_of_this_build_has_a_row() {
        assert!(
            app_version_for_schema(migrations_current_version()).is_some(),
            "schema {} has no row in SCHEMA_APP_VERSIONS: add the app version that ships it",
            migrations_current_version()
        );
    }

    #[test]
    fn rows_are_ordered_and_name_real_versions() {
        let mut last_schema = 0;
        let mut last_version = semver::Version::new(0, 0, 0);
        for (schema, version) in SCHEMA_APP_VERSIONS {
            assert!(*schema > last_schema, "schema {schema} out of order");
            let parsed = semver::Version::parse(version).expect("a plain release version");
            assert!(parsed.pre.is_empty() && parsed > last_version, "{version} out of order");
            assert!(*schema <= migrations_current_version());
            last_schema = *schema;
            last_version = parsed;
        }
    }

    #[test]
    fn unknown_schemas_give_no_guess() {
        assert_eq!(app_version_for_schema(52), Some("0.17.1"));
        // Development-only schemas: no release can read them.
        assert_eq!(app_version_for_schema(53), None);
        assert_eq!(app_version_for_schema(54), None);
        assert_eq!(app_version_for_schema(55), None);
        assert_eq!(app_version_for_schema(56), None);
        assert_eq!(app_version_for_schema(57), None);
        assert_eq!(app_version_for_schema(58), None);
        assert_eq!(app_version_for_schema(59), None);
        assert_eq!(app_version_for_schema(60), None);
        // 61 only existed in development builds; 0.18.0 ships with 62.
        assert_eq!(app_version_for_schema(61), None);
        assert_eq!(app_version_for_schema(62), Some("0.18.0"));
        assert_eq!(app_version_for_schema(0), None);
    }
}
