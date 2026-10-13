//! Workbench's template strings, read the way Workbench reads them.
//!
//! `page_title_template`, `rollback_config_filename_template` and
//! `rollback_csv_filename_template` are Python `string.Template`s (upstream
//! `workbench_utils.py`): `$name` takes the longest run of identifier characters, `${name}`
//! is braced, `$$` is a literal `$`, and an unknown name is an error at run time. The classic
//! mistake is `$config_filename_$input_csv_filename`, which Python reads as a placeholder named
//! `config_filename_` — the docs warn about it. These functions let the builder catch it, and
//! show what a template resolves to, before Workbench does.

/// One placeholder in a template, as Python would see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    pub name: String,
    /// Written `${name}` rather than `$name`.
    pub braced: bool,
}

/// The placeholders in `template`, or why Python would refuse it ("Invalid placeholder").
pub fn placeholders(template: &str) -> Result<Vec<Placeholder>, String> {
    let chars: Vec<char> = template.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            i += 1;
            continue;
        }
        match chars.get(i + 1) {
            Some('$') => i += 2,
            Some('{') => {
                let start = i + 2;
                let end = chars[start..]
                    .iter()
                    .position(|c| *c == '}')
                    .map(|n| start + n)
                    .ok_or_else(|| format!("The ${{ at position {} is never closed.", i + 1))?;
                let name: String = chars[start..end].iter().collect();
                if !is_identifier(&name) {
                    return Err(format!("\"${{{name}}}\" is not a valid placeholder name."));
                }
                out.push(Placeholder { name, braced: true });
                i = end + 1;
            }
            Some(c) if is_identifier_start(*c) => {
                let start = i + 1;
                let end = chars[start..]
                    .iter()
                    .position(|c| !is_identifier_char(*c))
                    .map_or(chars.len(), |n| start + n);
                out.push(Placeholder {
                    name: chars[start..end].iter().collect(),
                    braced: false,
                });
                i = end;
            }
            _ => {
                return Err(format!(
                    "The $ at position {} is not followed by a name; write $$ for a literal $.",
                    i + 1
                ));
            }
        }
    }
    Ok(out)
}

/// Problems with `template` given the placeholder names it may use, worded for the row: an
/// unknown name, with the `${…}` fix when it is a known name run into the following text.
pub fn check(template: &str, allowed: &[&str]) -> Option<String> {
    let found = match placeholders(template) {
        Ok(found) => found,
        Err(error) => return Some(error),
    };
    let unknown = found.iter().find(|p| !allowed.contains(&p.name.as_str()))?;
    // `$config_filename_x`: a legal name with text run into it.
    if !unknown.braced
        && let Some(known) = allowed
            .iter()
            .filter(|known| unknown.name.starts_with(**known))
            .max_by_key(|known| known.len())
    {
        let rest = &unknown.name[known.len()..];
        return Some(format!(
            "\"${}\" reads as one placeholder named \"{}\". Write \"${{{known}}}{rest}\".",
            unknown.name, unknown.name
        ));
    }
    Some(format!(
        "\"${}\" is not a placeholder here. Use {}.",
        unknown.name,
        allowed
            .iter()
            .map(|name| format!("${{{name}}}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// `template` with each placeholder replaced from `values`; `None` when it would not resolve.
pub fn render(template: &str, values: &[(&str, &str)]) -> Option<String> {
    let mut out = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    let lookup = |name: &str| values.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match chars.get(i + 1) {
            Some('$') => {
                out.push('$');
                i += 2;
            }
            Some('{') => {
                let start = i + 2;
                let end = start + chars[start..].iter().position(|c| *c == '}')?;
                let name: String = chars[start..end].iter().collect();
                out.push_str(lookup(&name)?);
                i = end + 1;
            }
            Some(c) if is_identifier_start(*c) => {
                let start = i + 1;
                let end = chars[start..]
                    .iter()
                    .position(|c| !is_identifier_char(*c))
                    .map_or(chars.len(), |n| start + n);
                let name: String = chars[start..end].iter().collect();
                out.push_str(lookup(&name)?);
                i = end;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// Python's `string.Template` identifier pattern is ASCII `[_a-z][_a-z0-9]*`, case-insensitive.
fn is_identifier_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_identifier_char(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(is_identifier_start) && chars.all(is_identifier_char)
}

/// The placeholders each template setting accepts, from upstream's `substitute(...)` calls.
pub fn allowed_placeholders(key: &str, task: Option<&str>) -> Option<&'static [&'static str]> {
    match key {
        "page_title_template" => Some(&["parent_title", "weight"]),
        // `create_from_files` substitutes only the config's name.
        "rollback_config_filename_template" | "rollback_csv_filename_template" => {
            Some(if task == Some("create_from_files") {
                &["config_filename"]
            } else {
                &[
                    "config_filename",
                    "input_csv_filename",
                    "csv_start_row",
                    "csv_stop_row",
                ]
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROLLBACK: &[&str] = &[
        "config_filename",
        "input_csv_filename",
        "csv_start_row",
        "csv_stop_row",
    ];

    #[test]
    fn python_reads_names_greedily() {
        let found = placeholders("my_${config_filename}_$input_csv_filename-$$x").unwrap();
        assert_eq!(
            found,
            vec![
                Placeholder {
                    name: "config_filename".into(),
                    braced: true
                },
                Placeholder {
                    name: "input_csv_filename".into(),
                    braced: false
                },
            ]
        );
        assert!(placeholders("cost $5").is_err());
        assert!(placeholders("${open").is_err());
    }

    /// The pitfall the docs warn about, with the fix spelled out.
    #[test]
    fn a_name_run_into_text_suggests_braces() {
        assert_eq!(
            check(
                "my-custom_filename_$config_filename_$input_csv_filename",
                ROLLBACK
            )
            .unwrap(),
            "\"$config_filename_\" reads as one placeholder named \"config_filename_\". \
             Write \"${config_filename}_\"."
        );
        assert!(
            check(
                "my-custom_filename_${config_filename}_$input_csv_filename",
                ROLLBACK
            )
            .is_none()
        );
        assert!(
            check("$title", ROLLBACK)
                .unwrap()
                .contains("${config_filename}")
        );
    }

    #[test]
    fn templates_render_like_python() {
        assert_eq!(
            render(
                "$parent_title, page $weight",
                &[("parent_title", "Book"), ("weight", "3")]
            ),
            Some("Book, page 3".into())
        );
        assert_eq!(render("$$${weight}", &[("weight", "1")]), Some("$1".into()));
        assert_eq!(render("$missing", &[]), None);
    }

    #[test]
    fn create_from_files_only_has_the_config_name() {
        assert_eq!(
            allowed_placeholders("rollback_csv_filename_template", Some("create_from_files")),
            Some(&["config_filename"][..])
        );
        assert_eq!(allowed_placeholders("nodes_only", None), None);
    }
}
