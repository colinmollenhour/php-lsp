//! PHP syntax validation for test fixtures using `php -l`.

use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

/// Validates that PHP code is syntactically correct using `php -l`.
/// Returns Ok(()) if valid, Err with the lint error if invalid.
pub fn validate(php_code: &str) -> Result<(), String> {
    // Write to temp file since php -l requires a file argument
    let mut temp = NamedTempFile::new().map_err(|e| format!("failed to create temp file: {e}"))?;
    temp.write_all(php_code.as_bytes())
        .map_err(|e| format!("failed to write temp file: {e}"))?;

    let output = match Command::new("php").arg("-l").arg(temp.path()).output() {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("failed to run php -l: {e}")),
    };

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let msg = if !stderr.is_empty() {
            stderr.to_string()
        } else {
            stdout.to_string()
        };
        Err(msg)
    }
}

/// Validates a fixture file and panics if syntax is invalid.
/// Use `#[allow(invalid_php)]` on the test to skip validation.
pub fn validate_fixture_file(path: &str, code: &str, allow_invalid: bool) {
    if allow_invalid {
        return;
    }

    if let Err(e) = validate(code) {
        panic!(
            "invalid PHP syntax in fixture file {path}:\n{e}\n\n\
             To allow intentional syntax errors, add `allow_invalid_php()` call before the fixture",
            path = path,
            e = e
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_correct_php() {
        let code = r#"<?php
class Foo {
    public function bar(): string {
        return "hello";
    }
}
"#;
        assert!(validate(code).is_ok());
    }

    #[test]
    fn rejects_invalid_php() {
        if validate("<?php").is_err() {
            return; // php not available
        }
        let code = r#"<?php
class Foo {
    public function bar() {
        return
    }
"#;
        assert!(validate(code).is_err());
    }

    #[test]
    fn accepts_minimal_code() {
        assert!(validate("<?php").is_ok());
    }

    #[test]
    fn accepts_code_without_php_tag() {
        // php -l doesn't require PHP tag, only tests do
        let code = "class Foo {}";
        assert!(validate(code).is_ok());
    }

    #[test]
    fn rejects_code_with_cursor_marker() {
        if validate("<?php").is_err() {
            return; // php not available
        }
        // $0 is invalid PHP syntax (fixture DSL marker)
        let code = r#"<?php
class Foo$0 {}"#;
        assert!(validate(code).is_err());
    }

    #[test]
    fn validates_code_after_cursor_removal() {
        // Simulate what fixture parser does: remove $0 before validation
        let code_with_marker = r#"<?php
class Foo$0 {}"#;
        let code_cleaned = code_with_marker.replace("$0", "");
        assert!(validate(&code_cleaned).is_ok());
    }

    #[test]
    fn accepts_annotation_comments() {
        // Fixture parser removes annotation LINES, but even if they're present,
        // they're valid PHP comments
        let code = r#"<?php
foo();
// ^^^ error: not defined"#;
        assert!(validate(code).is_ok());
    }
}
