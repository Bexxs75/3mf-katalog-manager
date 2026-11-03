use super::*;
use std::collections::HashSet;

const SHARED_LIST: &str = include_str!("../../../src/lib/errorCodes.json");

/// Codes that exist on one side only; empty when both sides agree.
fn list_differences(shared: &[String], registry: &[&str]) -> Vec<String> {
    let shared_set: HashSet<&str> = shared.iter().map(String::as_str).collect();
    let registry_set: HashSet<&str> = registry.iter().copied().collect();
    let mut diff: Vec<String> = shared_set.symmetric_difference(&registry_set).map(|c| c.to_string()).collect();
    diff.sort();
    diff
}

fn strings(value: &serde_json::Value, key: &str) -> Vec<String> {
    value[key].as_array().unwrap_or_else(|| panic!("{key} missing"))
        .iter().map(|v| v.as_str().expect("string").to_string()).collect()
}

/// The shared list the frontend test reads (`src/lib/errorCodes.json`).
fn shared_list_json() -> String {
    let list = |codes: Vec<&str>| {
        codes.iter().map(|c| format!("    \"{c}\"")).collect::<Vec<_>>().join(",\n")
    };
    format!(
        "{{\n  \"registered\": [\n{}\n  ],\n  \"legacy\": [\n{}\n  ]\n}}\n",
        list(ErrorCode::ALL.iter().map(|c| c.as_str()).collect()),
        list(LEGACY_CODES.to_vec()),
    )
}

#[test]
fn names_are_unique_camel_case() {
    let mut seen = HashSet::new();
    for code in ErrorCode::ALL {
        let name = code.as_str();
        assert!(seen.insert(name), "{name} is registered twice");
        assert!(name.chars().next().is_some_and(|c| c.is_ascii_lowercase()), "{name}");
        assert!(name.chars().all(|c| c.is_ascii_alphanumeric()), "{name}");
        assert!(!LEGACY_CODES.contains(&name), "{name} is also a legacy code");
    }
}

#[test]
fn shared_list_matches_the_registry() {
    let json: serde_json::Value = serde_json::from_str(SHARED_LIST).expect("errorCodes.json parses");
    let registered: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        list_differences(&strings(&json, "registered"), &registered), Vec::<String>::new(),
        "src/lib/errorCodes.json is out of date; run `cargo test regenerate_shared_list -- --ignored`"
    );
    assert_eq!(list_differences(&strings(&json, "legacy"), LEGACY_CODES), Vec::<String>::new());
}

/// A code the backend can emit but the shared list does not know shows up as a difference.
#[test]
fn an_unregistered_code_is_reported() {
    let shared = vec!["folderNameEmpty".to_string()];
    assert_eq!(list_differences(&shared, &["folderNameEmpty", "madeUpCode"]), vec!["madeUpCode".to_string()]);
    assert_eq!(list_differences(&["staleCode".to_string()], &[]), vec!["staleCode".to_string()]);
}

/// The check above compares two lists; this one proves it against the real file by
/// removing one registered code from the parsed list and expecting exactly that code
/// to be reported.
#[test]
fn dropping_a_code_from_the_shared_list_is_detected() {
    let json: serde_json::Value = serde_json::from_str(SHARED_LIST).expect("errorCodes.json parses");
    let mut shared = strings(&json, "registered");
    let removed = shared.pop().expect("the list is not empty");
    let registered: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(list_differences(&shared, &registered), vec![removed]);
}

/// Rewrites the shared list after a code was added; not part of the normal run.
#[test]
#[ignore]
fn regenerate_shared_list() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/lib/errorCodes.json");
    std::fs::write(path, shared_list_json()).unwrap();
}

/// The production code of a source file: everything except the items marked
/// `#[cfg(test)]` (test modules and test-only helpers), each skipped up to its
/// own end. Production code that follows a test-only helper stays in.
fn production_part(source: &str) -> String {
    let mut kept = Vec::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        if line.trim() != "#[cfg(test)]" {
            kept.push(line);
            continue;
        }
        // The item starts after its further attributes; `mod x;` ends on its own
        // line, anything else ends at the next closing brace in column 0.
        let mut item = lines.next();
        while item.is_some_and(|l| l.trim_start().starts_with("#[")) {
            item = lines.next();
        }
        if item.is_some_and(|l| !l.trim_end().ends_with(';')) {
            for rest in lines.by_ref() {
                if rest.starts_with('}') { break; }
            }
        }
    }
    kept.join("\n")
}

/// The body of `fn name`, up to the closing brace at column 0.
fn function_body(source: &str, name: &str) -> String {
    let production = production_part(source);
    let start = ["(", "<"].iter()
        .filter_map(|next| production.find(&format!("fn {name}{next}")))
        .min()
        .unwrap_or_else(|| panic!("fn {name} not found"));
    let rest = &production[start..];
    let end = rest.find("\n}").map_or(rest.len(), |i| i + 2);
    rest[..end].to_string()
}

/// A way to turn text into an expected error.
struct Sink {
    name: &'static str,
    /// Position of the argument that carries the text.
    text_arg: usize,
    /// A qualified constructor (`Type::function`), which is also refused when it is
    /// used as a value (`map_err(CmdError::expected)`). The bare helpers (`error`,
    /// `invalid`) are only recognised as calls, because a variable of that name is
    /// common and harmless.
    qualified: bool,
}

const SINKS: &[Sink] = &[
    Sink { name: "CmdError::expected", text_arg: 0, qualified: true },
    Sink { name: "WriteError::expected", text_arg: 0, qualified: true },
    Sink { name: "ReadError::expected", text_arg: 0, qualified: true },
    Sink { name: "ImportError::expected", text_arg: 0, qualified: true },
    Sink { name: "DbError::Invalid", text_arg: 0, qualified: true },
    // The server helpers take the status first and the message second.
    Sink { name: "error", text_arg: 1, qualified: false },
    Sink { name: "boxed_error", text_arg: 1, qualified: false },
    // The write path helper takes the message only.
    Sink { name: "invalid", text_arg: 0, qualified: false },
];

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The source without whole-line comments, so a sentence in a comment or doc line
/// that mentions a constructor is not mistaken for a call.
fn without_comment_lines(code: &str) -> String {
    code.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n")
}

/// The arguments of the call whose opening parenthesis starts `rest`, split at the
/// top level. String and character literals are skipped, so a comma or a bracket
/// inside text does not confuse the split. `None` when `rest` is not a call.
fn call_arguments(rest: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = rest.chars().collect();
    if chars.first() != Some(&'(') {
        return None;
    }
    let (mut depth, mut start, mut index) = (0usize, 1usize, 0usize);
    let mut arguments = Vec::new();
    while index < chars.len() {
        match chars[index] {
            '"' => {
                index += 1;
                while index < chars.len() && chars[index] != '"' {
                    if chars[index] == '\\' { index += 1; }
                    index += 1;
                }
            }
            '\'' if chars.get(index + 2) == Some(&'\'') => index += 2,
            '\'' if chars.get(index + 1) == Some(&'\\') && chars.get(index + 3) == Some(&'\'') => index += 3,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    arguments.push(chars[start..index].iter().collect::<String>());
                    break;
                }
            }
            ',' if depth == 1 => {
                arguments.push(chars[start..index].iter().collect());
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    // A trailing comma leaves an empty last argument.
    if arguments.last().is_some_and(|a| a.trim().is_empty()) {
        arguments.pop();
    }
    Some(arguments)
}

/// Every way a migrated function builds an expected error without a registered
/// code: the only text argument allowed is a literal that is one of the legacy
/// codes. Variables, constants, calls, `format!` and a constructor used as a value
/// (`map_err(CmdError::expected)`) are all reported, so nothing unregistered can
/// reach the interface through an indirect argument. A pattern such as
/// `DbError::Invalid(msg)` is reported too: a migrated file has no business
/// matching on the text variant.
fn free_text_errors(code: &str) -> Vec<String> {
    let code = without_comment_lines(code);
    let mut found = Vec::new();
    for sink in SINKS {
        for (index, _) in code.match_indices(sink.name) {
            let before = &code[..index];
            if before.chars().next_back().is_some_and(is_identifier_char) || before.ends_with("fn ") {
                continue;
            }
            let rest = &code[index + sink.name.len()..];
            if !sink.qualified && !rest.starts_with('(') {
                continue;
            }
            let allowed = call_arguments(rest).is_some_and(|arguments| {
                arguments.get(sink.text_arg).is_none_or(|text| {
                    LEGACY_CODES.iter().any(|legacy| text.trim_start() == format!("\"{legacy}\""))
                })
            });
            if !allowed {
                found.push(format!("{}{}", sink.name, rest.lines().next().unwrap_or_default()));
            }
        }
    }
    found
}

/// Modules (or single functions) whose expected errors are fully migrated.
/// `files.rs` and `companions.rs` hold unrelated actions too, so only the
/// functions of the migrated actions are scanned there.
fn migrated_sources() -> Vec<(&'static str, String)> {
    let whole = |name: &'static str, source: &str| (name, production_part(source));
    let functions = |name: &'static str, source: &str, fns: &[&str]| {
        (name, fns.iter().map(|f| function_body(source, f)).collect::<Vec<_>>().join("\n"))
    };
    vec![
        whole("folders.rs", include_str!("../../../src-tauri/src/commands/folders.rs")),
        whole("folder_trash.rs", include_str!("../../../src-tauri/src/commands/folder_trash.rs")),
        whole("trash.rs", include_str!("../../../src-tauri/src/commands/trash.rs")),
        whole("catalog_removal.rs", include_str!("../../../src-tauri/src/commands/catalog_removal.rs")),
        whole("source_folders.rs", include_str!("../../../src-tauri/src/commands/source_folders.rs")),
        functions("files.rs", include_str!("../../../src-tauri/src/commands/files.rs"), &[
            "validate_file_name", "move_file_core", "move_files_to_folder_with", "rename_file_with_conn",
        ]),
        functions("companions.rs", include_str!("../../../src-tauri/src/commands/companions.rs"), &["carry_with", "extras_dir_problem"]),
        functions("backup.rs", include_str!("../../../src-tauri/src/commands/backup.rs"), &[
            "reject_oversized_zip_entry", "read_catalog_archive", "validate_catalog_db_bytes", "validate_backup_for_import",
        ]),
    ]
}

#[test]
fn migrated_code_emits_no_free_text() {
    for (name, code) in migrated_sources() {
        assert_eq!(free_text_errors(&code), Vec::<String>::new(), "{name} still emits free text");
    }
}

/// Production code after a test-only helper must still be scanned.
#[test]
fn production_code_after_a_test_only_helper_is_scanned() {
    let fixture = "fn a() {}\n#[cfg(test)]\npub(super) fn helper() -> u8 {\n    CmdError::expected(\"Nur im Test\");\n    1\n}\n\npub(super) fn production() {\n    return Err(CmdError::expected(\"Es fehlt etwas.\"));\n}\n#[cfg(test)]\n#[path = \"x_tests.rs\"]\nmod tests;\n";
    let production = production_part(fixture);
    assert!(!production.contains("Nur im Test"), "{production}");
    assert_eq!(free_text_errors(&production), vec!["CmdError::expected(\"Es fehlt etwas.\"));".to_string()]);
    let with_module = "fn b() {}\n#[cfg(test)]\nmod tests {\n    fn t() { CmdError::expected(\"Test\"); }\n}\nfn c() { CmdError::expected(\"Danach\"); }\n";
    assert_eq!(free_text_errors(&production_part(with_module)), vec!["CmdError::expected(\"Danach\"); }".to_string()]);
}

/// An unregistered code that is emitted through a variable must fail the real
/// contract check, the one the migrated modules are held to.
#[test]
fn an_unregistered_code_emitted_through_a_variable_fails_the_contract() {
    let fixture = "fn migrated() -> CmdResult<()> {\n    let problem = \"unregisteredCode\";\n    Err(CmdError::expected(problem))\n}\n";
    let violations = free_text_errors(&production_part(fixture));
    assert_eq!(violations, vec!["CmdError::expected(problem))".to_string()]);
    let via_constant = "const CODE: &str = \"alsoUnregistered\";\nfn f() { let _ = CmdError::expected(CODE); }\n";
    assert_eq!(free_text_errors(&production_part(via_constant)).len(), 1);
}

/// The scanner itself must fail on free text and accept registered and legacy codes.
#[test]
fn the_scanner_flags_free_text() {
    assert_eq!(free_text_errors(r#"Err(CmdError::expected("Ordner existiert bereits"))"#).len(), 1);
    assert_eq!(free_text_errors(r#"CmdError::expected(format!("x {}", 1))"#).len(), 1);
    assert!(free_text_errors(r#"CmdError::expected("cancelled")"#).is_empty());
    assert!(free_text_errors("CmdError::coded(ErrorCode::FolderNameEmpty)").is_empty());
    // Indirect arguments cannot be told apart from free text, so they are refused too.
    assert_eq!(free_text_errors("CmdError::expected(e.to_string())").len(), 1);
    assert_eq!(free_text_errors("map_err(CmdError::expected)").len(), 1);
    assert_eq!(free_text_errors(r#"CmdError::expected("cancelled" )"#).len(), 1);
}

/// The constructors of the core error types and the server helpers are held to the
/// same rule as `CmdError::expected`.
#[test]
fn the_scanner_covers_the_core_and_server_constructors() {
    for text in [
        r#"WriteError::expected("Ungültiger Farbwert")"#,
        r#"ReadError::expected("Datei überschreitet das Limit")"#,
        r#"ImportError::expected("Der Name darf nicht leer sein")"#,
        r#"DbError::Invalid(format!("{label} darf höchstens {max} Zeichen enthalten"))"#,
        r#"return Err(invalid("Ungültiger Farbwert"))"#,
        r#"invalid(&format!("Ungültig: {x}"))"#,
        r#"web::error(401, "Anmeldung erforderlich")"#,
        r#"boxed_error(429, "Zu viele Fehlversuche")"#,
        r#"error(StatusCode::BAD_REQUEST, "Ungültige Argumente", true)"#,
        // The text may sit on a line of its own.
        "error(\n    400,\n    \"Ungültige Argumente\",\n)",
    ] {
        assert_eq!(free_text_errors(text).len(), 1, "{text}");
    }
    // Indirect arguments are refused for the helpers too.
    for indirect in [
        "error(500, message)",
        "boxed_error(500, &text)",
        "invalid(message)",
        "WriteError::expected(CODE)",
        "ImportError::expected(e.to_string())",
        "map_err(ReadError::expected)",
    ] {
        assert_eq!(free_text_errors(indirect).len(), 1, "{indirect}");
    }
    // A comma inside the text does not move the text to another argument.
    assert_eq!(free_text_errors(r#"error(400, "Eins, zwei (drei)")"#).len(), 1);
}

/// What the scanner has to leave alone, so a migrated file can use the helpers for
/// status-only responses and registered codes.
#[test]
fn the_scanner_accepts_code_only_calls_and_unrelated_names() {
    for harmless in [
        "error(404)",
        "boxed_error(500)",
        r#"error(400, "cancelled")"#,
        r#"invalid("cancelled")"#,
        "error_coded(400, ErrorCode::WebNotFound, &[])",
        "WriteError::coded(ErrorCode::InvalidCatalogId)",
        "DbError::coded(ErrorCode::InvalidCatalogId)",
        "ReadError::coded_with(ErrorCode::FolderNotFound, [\"a\"])",
        // A name that merely ends like a helper is not the helper.
        r#"io_error("Zeile")"#,
        r#"sql_error("Zeile")"#,
        r#"is_invalid("Zeile")"#,
        // The definition of a helper and comments are not calls.
        "fn invalid(message: &str) -> WriteError {",
        "pub(crate) fn error(status: u16, message: &str) -> Response {",
        "// error(400, \"Beispiel\") is the old form",
        "/// Use invalid(\"text\") only in tests",
        // A variable called `error` is not a call.
        "Err(error) => return error.into(),",
    ] {
        assert_eq!(free_text_errors(harmless), Vec::<String>::new(), "{harmless}");
    }
}

/// The argument splitter on the shapes the sources really contain.
#[test]
fn call_arguments_splits_at_the_top_level_only() {
    assert_eq!(call_arguments("(a, b)").unwrap(), vec!["a", " b"]);
    assert_eq!(call_arguments(r#"(404, "x, y")"#).unwrap(), vec!["404", r#" "x, y""#]);
    assert_eq!(call_arguments("(f(a, b), c)").unwrap().len(), 2);
    assert_eq!(call_arguments(r#"(1, "a\"b", 'x', '"')"#).unwrap().len(), 4);
    assert_eq!(call_arguments("(a,\n b,\n)").unwrap().len(), 2);
    assert_eq!(call_arguments("()").unwrap().len(), 0);
    assert!(call_arguments(" (a)").is_none());
}
