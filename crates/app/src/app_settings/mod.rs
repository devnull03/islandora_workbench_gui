//! Islandora Workbench–specific settings pages (mockup `3b`).
//!
//! **Pages are named after what you are doing**, and split along one line: facts about *this
//! machine* (where Python lives) sit apart from anything *institutional* (which servers, which
//! configs), because Stage 5 bundles the second kind into a switchable profile and must not drag
//! the first kind along. See `docs/plans/stage-5-profiles.md`.

mod custom_fields;

use gpui_component::setting::{SettingGroup, SettingItem, SettingPage};

use settings::{Setting, picker_with_path_button};

use custom_fields::{saved_configs_field, saved_servers_field};

fn exe_candidates(base: &'static str) -> Vec<&'static str> {
    if cfg!(windows) {
        // Avoid concat! here because `base` is not a literal.
        if base == "python" {
            vec!["python.exe", "python"]
        } else if base == "uv" {
            vec!["uv.exe", "uv"]
        } else {
            vec![base]
        }
    } else {
        vec![base]
    }
}

/// Does this item answer to the query? Matching is over the words the user can actually see —
/// label, description and the setting key — so searching `uv` finds "Use UV" and searching
/// `credentials` finds the server list.
fn hit(query: &str, terms: &[&str]) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let q = query.trim().to_lowercase();
    terms.iter().any(|t| t.to_lowercase().contains(&q))
}

/// One group: its title, and its items paired with the words each answers to.
type Group = (&'static str, Vec<(&'static [&'static str], SettingItem)>);

/// Build a page, keeping only what matches. Page title, group title and item terms are weighed
/// together — searching a page's name has to show that whole page, which a group-at-a-time
/// filter cannot do because by then the page title is out of scope.
///
/// `None` when nothing survived: an empty page is not rendered at all, which is what makes the
/// filter read as "narrow everything down" rather than "grey some things out".
fn page(query: &str, title: &'static str, groups: Vec<Group>) -> Option<SettingPage> {
    let page_hit = hit(query, &[title]);
    let kept: Vec<SettingGroup> = groups
        .into_iter()
        .filter_map(|(group_title, items)| {
            let keep_all = page_hit || hit(query, &[group_title]);
            let items: Vec<SettingItem> = items
                .into_iter()
                .filter(|(terms, _)| keep_all || hit(query, terms))
                .map(|(_, item)| item)
                .collect();
            (!items.is_empty()).then(|| SettingGroup::new().title(group_title).items(items))
        })
        .collect();
    (!kept.is_empty()).then(|| SettingPage::new(title).default_open(true).groups(kept))
}

pub fn build_pages(query: &str) -> Vec<SettingPage> {
    let q = query;
    [
        page(
            q,
            "General",
            vec![(
                "Runs",
                vec![(
                    &["auto-accept prompts", "auto_accept_prompts", "confirm"],
                    Setting::Switch {
                        key: "auto_accept_prompts",
                        label: "Auto-accept prompts",
                        description: "Answer Workbench's confirmation prompts automatically. \
                                      Servers marked \"always confirm\" still prompt.",
                    }
                    .into(),
                )],
            )],
        ),
        // Machine facts. Nothing here belongs to an institution, so nothing here moves when
        // profiles arrive.
        page(
            q,
            "Workbench & Python",
            vec![
                (
                    "Workbench Installation",
                    vec![(
                        &["workbench path", "workbench_path", "installation"],
                        Setting::DirPicker {
                            key: "workbench_path",
                            label: "Workbench Path",
                            description: "Path to the Islandora Workbench installation directory",
                            prompt: "Select workbench directory",
                        }
                        .into(),
                    )],
                ),
                (
                    "Python Environment",
                    vec![
                        (
                            &["python path", "python_path", "interpreter"],
                            picker_with_path_button(
                                "python_path",
                                "Python Path",
                                "Path to the Python executable",
                                "Select Python executable",
                                {
                                    let mut c = exe_candidates("python");
                                    if !cfg!(windows) {
                                        c.push("python3");
                                    }
                                    c
                                },
                            ),
                        ),
                        (
                            &["uv path", "uv_path", "package manager"],
                            picker_with_path_button(
                                "uv_path",
                                "UV Path",
                                "Path to the UV package manager (optional)",
                                "Select UV executable",
                                exe_candidates("uv"),
                            ),
                        ),
                        (
                            &["use uv", "use_uv"],
                            Setting::Switch {
                                key: "use_uv",
                                label: "Use UV",
                                description: "Run Workbench through UV instead of Python directly",
                            }
                            .into(),
                        ),
                    ],
                ),
            ],
        ),
        page(
            q,
            "Servers",
            vec![(
                "Servers",
                vec![(
                    &["server", "host", "credentials", "url", "test"],
                    saved_servers_field(),
                )],
            )],
        ),
        page(
            q,
            "Config library",
            vec![(
                "Saved configurations",
                vec![(
                    &["config", "task", "yaml", "library"],
                    saved_configs_field(),
                )],
            )],
        ),
        page(
            q,
            "Input & preprocess",
            vec![(
                "Preprocess scripts",
                vec![(
                    &[
                        "preprocess",
                        "scripts folder",
                        "preprocess_scripts_dir",
                        "python script",
                    ],
                    Setting::DirPicker {
                        key: "preprocess_scripts_dir",
                        label: "Scripts Folder",
                        description: "Every .py file here is offered as a preprocess script. \
                                      A script is called with --input, --output-dir and \
                                      optionally --config, and must write a CSV.",
                        prompt: "Select preprocess scripts folder",
                    }
                    .into(),
                )],
            )],
        ),
        // Every default the config builder had to hard-code in Stage 1 becomes a setting here.
        page(
            q,
            "Config builder",
            vec![(
                "Defaults",
                vec![
                    (
                        &["save new configs to", "config_library_dir", "library folder"],
                        Setting::DirPicker {
                            key: "config_library_dir",
                            label: "Save new configs to",
                            description:
                                "Where \"Save to library\" writes a config that has no file yet",
                            prompt: "Select the config library folder",
                        }
                        .into(),
                    ),
                    (
                        &["yaml panel", "builder_show_yaml", "preview"],
                        Setting::Switch {
                            key: "builder_show_yaml",
                            label: "Show the YAML panel",
                            description: "Open the builder with the read-only YAML preview visible",
                        }
                        .into(),
                    ),
                ],
            )],
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The filter has to narrow, not empty. An empty query must show everything, a term that
    /// matches nothing must show nothing, and a page must survive on a term that only one of its
    /// items answers to.
    #[test]
    fn search_narrows_the_pages() {
        let all = build_pages("");
        assert_eq!(all.len(), 6, "every page shows when nothing is typed");

        assert!(build_pages("zzzznope").is_empty());

        // `uv` lives in one group of one page.
        assert_eq!(build_pages("uv").len(), 1);
        // A page title matches even when no item text does.
        assert_eq!(build_pages("Config library").len(), 1);
        // Case-insensitive, and matches a key as well as a label.
        assert_eq!(build_pages("WORKBENCH_PATH").len(), 1);
    }
}
