//! Set a few top-level scalar keys in a YAML document's *text*, leaving every other byte alone.
//!
//! The run-time rewrite used to re-serialize the whole config through `serde_yaml`, which drops
//! comments, rewrites quoting and reorders nothing-in-particular — on every Check and Run, in a
//! file the user may keep under version control. Only `host`, `credentials_file_path` and
//! `input_csv` ever change there, so only their lines are touched (plan task 1.3, question Q3).
//!
//! Deliberately narrow: a block mapping at the top level, one document. Anything else returns
//! `None` and the caller falls back to re-serializing. The caller also re-parses the result and
//! compares it against the intent, so a document this misreads cannot be written.

/// Return `text` with each `(key, value)` set as a top-level plain-string entry: an existing
/// entry's line (and any indented continuation lines) is replaced in place; a missing key is
/// appended. `None` when the document is not something this can patch safely.
pub(crate) fn set_top_level_scalars(text: &str, entries: &[(&str, &str)]) -> Option<String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();

    if !is_plain_block_mapping(&lines) {
        return None;
    }

    for (key, value) in entries {
        let replacement = format!("{key}: {}{newline}", render_scalar(value)?);
        match find_entry(&lines, key) {
            Some((start, end)) => {
                lines.splice(start..end, [replacement]);
            }
            None => {
                if let Some(last) = lines.last_mut()
                    && !last.ends_with('\n')
                {
                    last.push_str(newline);
                }
                lines.push(replacement);
            }
        }
    }
    Some(lines.concat())
}

/// Return `text` with one top-level entry replaced by `key: value` in block style — or removed
/// when `value` is `None` — leaving every other line alone. For values that are not scalars
/// (a `secondary_tasks` list); `None` when the document is not something this can patch.
pub(crate) fn set_top_level_value(
    text: &str,
    key: &str,
    value: Option<&serde_yaml::Value>,
) -> Option<String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
    if !is_plain_block_mapping(&lines) {
        return None;
    }
    let replacement: Vec<String> = match value {
        Some(value) => {
            let mut one = serde_yaml::Mapping::new();
            one.insert(key.into(), value.clone());
            serde_yaml::to_string(&one)
                .ok()?
                .lines()
                .map(|line| format!("{line}{newline}"))
                .collect()
        }
        None => Vec::new(),
    };
    match find_entry(&lines, key) {
        Some((start, end)) => {
            lines.splice(start..end, replacement);
        }
        None => {
            if let Some(last) = lines.last_mut()
                && !last.ends_with('\n')
            {
                last.push_str(newline);
            }
            lines.extend(replacement);
        }
    }
    Some(lines.concat())
}

/// One document whose top level is a block mapping. A flow mapping (`{task: create}`), a
/// second document, or an explicit end marker are all legal YAML this does not attempt.
fn is_plain_block_mapping(lines: &[String]) -> bool {
    let mut seen_content = false;
    for line in lines {
        let bare = line.trim_end_matches(['\r', '\n']);
        if bare.trim().is_empty() || bare.trim_start().starts_with('#') {
            continue;
        }
        if bare.starts_with("...") {
            return false;
        }
        if let Some(after) = bare.strip_prefix("---") {
            if seen_content {
                return false;
            }
            // A leading `---` with content on the same line is a document whose root is not
            // a block mapping.
            if !after.trim().is_empty() && !after.trim_start().starts_with('#') {
                return false;
            }
            continue;
        }
        if !seen_content && (bare.starts_with('{') || bare.starts_with('[')) {
            return false;
        }
        seen_content = true;
    }
    true
}

/// The line range `[start, end)` holding `key`'s top-level entry: its key line plus every
/// following indented or blank line, less any trailing blank lines (which separate entries
/// and belong to the layout, not the value).
fn find_entry(lines: &[String], key: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| is_key_line(line, key))?;
    let mut end = start + 1;
    while end < lines.len() {
        let line = &lines[end];
        let blank = line.trim().is_empty();
        let indented = line.starts_with([' ', '\t']);
        // A block sequence may sit at its key's own indent (`key:` then `- a` at column 0),
        // which is how serde_yaml writes one.
        let sequence_item = line == "-"
            || line.starts_with("- ")
            || line.starts_with("-\n")
            || line.starts_with("-\r\n");
        if !(blank || indented || sequence_item) {
            break;
        }
        end += 1;
    }
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    Some((start, end))
}

/// A column-0 `key:` (plain, single- or double-quoted). Column 0 is what makes it top level:
/// in a block mapping nothing else, including block-scalar content, can start there.
fn is_key_line(line: &str, key: &str) -> bool {
    [key.to_string(), format!("'{key}'"), format!("\"{key}\"")]
        .iter()
        .any(|spelling| {
            line.strip_prefix(spelling.as_str())
                .map(str::trim_start)
                .and_then(|rest| rest.strip_prefix(':'))
                .is_some_and(|after| after.is_empty() || after.starts_with([' ', '\t', '\r', '\n']))
        })
}

/// `value` as a one-line YAML scalar that reads back as the same string under **both** YAML
/// 1.2 and YAML 1.1.
///
/// Current Workbench loads configs with `ruamel.yaml` (1.2), but older releases used PyYAML
/// (1.1), where a bare `yes`, `on` or `1:20` is a boolean or a sexagesimal number — and that
/// is what serde_yaml's own quoting (1.2 rules) would leave bare. Plain only for an
/// unmistakable path- or URL-like string; everything else is single-quoted, which every
/// version reads as a string.
fn render_scalar(value: &str) -> Option<String> {
    if value.contains(['\n', '\r']) {
        // None of the keys patched here can legitimately hold a line break.
        return None;
    }
    Some(if is_safe_plain(value) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "''"))
    })
}

fn is_safe_plain(value: &str) -> bool {
    const YAML_1_1_WORDS: &[&str] = &[
        "y", "yes", "n", "no", "true", "false", "on", "off", "null", "~",
    ];
    let first_ok = value
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || "_./\\~".contains(c));
    let rest_ok = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_./\\~:@+=?%-".contains(c));
    // Anything made only of digits and number punctuation might be read as a number (1.1
    // reads `1:20` as 80); a trailing `:` would read as a mapping key.
    let numeric_looking = value
        .chars()
        .all(|c| c.is_ascii_digit() || "._:+-eE".contains(c));
    first_ok
        && rest_ok
        && !numeric_looking
        && !value.ends_with(':')
        && !YAML_1_1_WORDS.contains(&value.to_ascii_lowercase().as_str())
}

/// Whether the document's text carries comments — what a full re-serialization would lose.
///
/// A heuristic: a `#` at the start of a line or after whitespace. A quoted string containing
/// ` #` reads as a comment too; that only ever over-warns, never under-warns.
pub fn has_comments(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with('#') || line.contains(" #") || line.contains("\t#")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HAND_WRITTEN: &str = "\
# Production ingest — reviewed by the metadata team
task: create
host: \"https://old.example.org\"   # replaced at run time
input_dir: 'input_data'

# Media handling
media_types:
  - file_extension: [tif, tiff]
    type: image

credentials_file_path: |
  /old/creds.yml
log_file_path: logs/run.log
";

    #[test]
    fn replaces_only_the_named_lines_and_keeps_everything_else_byte_identical() {
        let patched = set_top_level_scalars(
            HAND_WRITTEN,
            &[
                ("host", "https://new.example.org"),
                ("credentials_file_path", "/new/creds.yml"),
            ],
        )
        .unwrap();

        let expected = HAND_WRITTEN
            .replace(
                "host: \"https://old.example.org\"   # replaced at run time\n",
                "host: https://new.example.org\n",
            )
            .replace(
                "credentials_file_path: |\n  /old/creds.yml\n",
                "credentials_file_path: /new/creds.yml\n",
            );
        assert_eq!(patched, expected);
    }

    #[test]
    fn appends_a_missing_key_and_quotes_when_yaml_needs_it() {
        let patched =
            set_top_level_scalars("task: create # main\nid_field: id", &[("input_csv", "yes")])
                .unwrap();
        assert_eq!(
            patched,
            "task: create # main\nid_field: id\ninput_csv: 'yes'\n"
        );
    }

    #[test]
    fn values_are_quoted_whenever_either_yaml_version_could_misread_them() {
        for plain in [
            "https://islandora.dev",
            "/home/me/creds.yml",
            "C:\\data\\metadata.csv",
            "input_data/metadata.csv",
        ] {
            assert_eq!(render_scalar(plain).as_deref(), Some(plain), "{plain}");
        }
        for (raw, quoted) in [
            ("yes", "'yes'"),
            ("On", "'On'"),
            ("1:20", "'1:20'"),
            ("12", "'12'"),
            ("", "''"),
            ("my creds.yml", "'my creds.yml'"),
            ("it's", "'it''s'"),
            ("#x", "'#x'"),
        ] {
            assert_eq!(render_scalar(raw).as_deref(), Some(quoted), "{raw}");
        }
        assert_eq!(render_scalar("a\nb"), None);
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let patched = set_top_level_scalars("task: create\r\nhost: a\r\n", &[("host", "b")]);
        assert_eq!(patched.as_deref(), Some("task: create\r\nhost: b\r\n"));
    }

    #[test]
    fn indented_keys_with_the_same_name_are_not_top_level() {
        let text = "task: create\nfield_templates:\n  host: keep-me\n";
        let patched = set_top_level_scalars(text, &[("host", "new")]).unwrap();
        assert_eq!(patched, format!("{text}host: new\n"));
    }

    #[test]
    fn refuses_documents_it_cannot_patch_safely() {
        assert!(set_top_level_scalars("{task: create}\n", &[("host", "x")]).is_none());
        assert!(set_top_level_scalars("task: a\n---\ntask: b\n", &[("host", "x")]).is_none());
        assert!(set_top_level_scalars("task: a\n...\n", &[("host", "x")]).is_none());
        // A leading document marker on its own is fine.
        assert!(set_top_level_scalars("---\ntask: a\n", &[("host", "x")]).is_some());
    }

    #[test]
    fn comment_detection() {
        assert!(has_comments(HAND_WRITTEN));
        assert!(!has_comments("task: create\nhost: https://x.org/#frag\n"));
    }
}
