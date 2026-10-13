//! Settings → Profiles (mockup `3c`): a card per profile with Switch to · Rename · Duplicate ·
//! Export… · ✕, then New profile and Import…, and the line between what a profile holds and what
//! stays with this machine.

use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Disableable, IconName, Sizable, h_flex,
    input::{Input, InputState},
    label::Label,
    setting::SettingItem,
    v_flex,
};

use settings::AppSettings;
use settings::profiles::{self, Profile};
use ui::tokens::{GAP_MD, GAP_SM};
use ui::{APP_CONTROL_SIZE, Card, CardTone, app_button, app_tag, ghost_button};
use window_wrapper::WindowLock;

/// The profile whose name is being edited, if any.
#[derive(Default)]
struct Renaming(Option<String>);

/// A message about the last profile action that failed (a name already taken, an unreadable
/// import), shown under the list.
#[derive(Default)]
struct LastError(Option<SharedString>);

pub fn profiles_field() -> SettingItem {
    SettingItem::render(|_options, window, cx| {
        let renaming =
            window.use_keyed_state(SharedString::from("profile-renaming"), cx, |_, _| {
                Renaming::default()
            });
        let error = window.use_keyed_state(SharedString::from("profile-error"), cx, |_, _| {
            LastError::default()
        });
        let settings = AppSettings::get(cx);
        let active = settings.active_profile.clone();
        let all = settings.all_profiles();
        // A switch mid-run would change the server and config under a running Workbench.
        let locked = WindowLock::is_locked(cx);

        let cards: Vec<AnyElement> = all
            .iter()
            .map(|profile| profile_card(profile, &active, locked, &renaming, &error, window, cx))
            .collect();
        let message = error.read(cx).0.clone();

        v_flex()
            .w_full()
            .gap(GAP_MD)
            .children(cards)
            .child(
                h_flex()
                    .gap(GAP_SM)
                    .child(
                        app_button("new-profile")
                            .outline()
                            .icon(IconName::Plus)
                            .label("New profile")
                            .on_click({
                                let renaming = renaming.clone();
                                let error = error.clone();
                                move |_, _, cx| {
                                    let added = AppSettings::update(cx, |s| {
                                        s.add_profile("New profile", None)
                                    });
                                    report(&error, added.as_ref().err(), cx);
                                    if let Ok(name) = added {
                                        renaming.update(cx, |r, _| r.0 = Some(name));
                                    }
                                }
                            }),
                    )
                    .child(
                        app_button("import-profile")
                            .outline()
                            .label("Import…")
                            .on_click({
                                let error = error.clone();
                                move |_, _, cx| import_profile(error.clone(), cx)
                            }),
                    ),
            )
            .children(message.map(ui::InlineMessage::error))
            .child(explainer(cx))
    })
}

#[allow(clippy::too_many_arguments)]
fn profile_card(
    profile: &Profile,
    active: &str,
    locked: bool,
    renaming: &Entity<Renaming>,
    error: &Entity<LastError>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let name = profile.name.clone();
    let is_active = name == active;
    let editing = renaming.read(cx).0.as_deref() == Some(name.as_str());
    let muted = cx.theme().colors.muted_foreground;
    let id = |what: &str| SharedString::from(format!("profile-{what}-{name}"));

    let title: AnyElement = if editing {
        let input = window.use_keyed_state(id("rename-input"), cx, {
            let name = name.clone();
            move |window, cx| InputState::new(window, cx).default_value(name)
        });
        h_flex()
            .gap(GAP_SM)
            .flex_1()
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&input).with_size(APP_CONTROL_SIZE)),
            )
            .child(
                app_button(id("rename-save"))
                    .outline()
                    .label("Save")
                    .on_click({
                        let (from, input, renaming, error) =
                            (name.clone(), input.clone(), renaming.clone(), error.clone());
                        move |_, _, cx| {
                            let to = input.read(cx).value().to_string();
                            let result = AppSettings::update(cx, |s| s.rename_profile(&from, &to));
                            report(&error, result.as_ref().err(), cx);
                            if result.is_ok() {
                                renaming.update(cx, |r, _| r.0 = None);
                            }
                        }
                    }),
            )
            .into_any_element()
    } else {
        h_flex()
            .gap(GAP_SM)
            .flex_1()
            .items_center()
            .child(Label::new(name.clone()).text_sm())
            .when(is_active, |row| {
                row.child(app_tag().child(Label::new("active").text_xs()))
            })
            .into_any_element()
    };

    let actions = h_flex()
        .gap(GAP_SM)
        .items_center()
        .when(!is_active, |row| {
            let (target, error) = (name.clone(), error.clone());
            row.child(
                ghost_button(id("switch"))
                    .label("Switch to")
                    .disabled(locked)
                    .tooltip(if locked {
                        "Wait for the run to finish"
                    } else {
                        "Use this profile's servers, configs and input settings"
                    })
                    .on_click(move |_, _, cx| {
                        let result = AppSettings::update(cx, |s| s.switch_profile(&target));
                        report(&error, result.as_ref().err(), cx);
                    }),
            )
        })
        .when(!editing, |row| {
            let (target, renaming) = (name.clone(), renaming.clone());
            row.child(
                ghost_button(id("rename"))
                    .label("Rename")
                    .on_click(move |_, _, cx| {
                        renaming.update(cx, |r, _| r.0 = Some(target.clone()))
                    }),
            )
        })
        .child({
            let (source, error) = (name.clone(), error.clone());
            ghost_button(id("duplicate"))
                .label("Duplicate")
                .on_click(move |_, _, cx| {
                    let result = AppSettings::update(cx, |s| s.add_profile(&source, Some(&source)));
                    report(&error, result.as_ref().err(), cx);
                })
        })
        .child({
            let profile = profile.clone();
            ghost_button(id("export"))
                .label("Export…")
                .tooltip("Save as a .profile.yml — paths and settings, never passwords")
                .on_click(move |_, _, cx| export_profile(profile.clone(), cx))
        })
        .child({
            let (target, error) = (name.clone(), error.clone());
            ghost_button(id("delete"))
                .icon(IconName::Close)
                .disabled(is_active)
                .tooltip(if is_active {
                    "Switch to another profile to delete this one"
                } else {
                    "Delete this profile"
                })
                .on_click(move |_, window, cx| {
                    let (target, error) = (target.clone(), error.clone());
                    ui::confirm_dialog(
                        window,
                        cx,
                        format!("Delete the {target} profile?"),
                        "Its server list, config list and input settings go. The config files, \
                         credentials files and scripts on disk are not touched.",
                        "Delete",
                        move |_, cx| {
                            let result = AppSettings::update(cx, |s| s.delete_profile(&target));
                            report(&error, result.as_ref().err(), cx);
                        },
                    );
                })
        });

    Card::new()
        .tone(CardTone::Filled)
        .gap(GAP_SM)
        .child(
            h_flex()
                .w_full()
                .gap(GAP_MD)
                .items_center()
                .child(title)
                .child(actions),
        )
        .child(
            Label::new(profiles::summary(profile))
                .text_xs()
                .text_color(muted),
        )
        .into_any_element()
}

/// "In the profile" / "On this machine", as `3c` draws it, plus the one note about export.
fn explainer(cx: &App) -> impl IntoElement {
    let muted = cx.theme().colors.muted_foreground;
    let column = |title: &'static str, lines: &[&'static str]| {
        v_flex()
            .flex_1()
            .gap(GAP_SM)
            .child(Label::new(title).text_xs().text_color(muted))
            .children(lines.iter().map(|line| Label::new(*line).text_xs()))
    };
    v_flex()
        .w_full()
        .gap(GAP_MD)
        .child(
            h_flex()
                .w_full()
                .gap(GAP_MD)
                .items_start()
                .child(column(
                    "IN THE PROFILE",
                    &[
                        "Servers and their credentials files",
                        "Config library folder and saved configs",
                        "Input source and its values, ingest folder",
                        "Preprocess scripts folder",
                        "Builder defaults, auto-accept",
                    ],
                ))
                .child(column(
                    "ON THIS MACHINE",
                    &[
                        "Workbench folder, Python, uv",
                        "Theme and window sizes",
                        "Update checks",
                    ],
                )),
        )
        .child(ui::InlineMessage::info(
            "An exported profile names credentials files by path; it never contains a password.",
        ))
}

fn report(error: &Entity<LastError>, failure: Option<&String>, cx: &mut App) {
    let message = failure.map(|e| SharedString::from(e.clone()));
    error.update(cx, |e, _| e.0 = message);
}

fn export_profile(profile: Profile, cx: &mut App) {
    let suggested = format!(
        "{}.profile.yml",
        profile.name.replace(['/', '\\', ':'], "_")
    );
    let directory = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
    cx.spawn(async move |_| {
        if let Ok(Ok(Some(path))) = receiver.await
            && let Ok(text) = profiles::export_yaml(&profile)
            && let Err(e) = std::fs::write(&path, text)
        {
            log::error!("[profiles] export to {} failed: {e}", path.display());
        }
    })
    .detach();
}

fn import_profile(error: Entity<LastError>, cx: &mut App) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Import a profile".into()),
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = receiver.await else {
            return;
        };
        let Some(path) = paths.first() else {
            return;
        };
        let parsed = std::fs::read_to_string(path)
            .map_err(|e| format!("Couldn't read {}: {e}", path.display()))
            .and_then(|text| profiles::import_yaml(&text));
        cx.update(|cx| match parsed {
            Ok(profile) => {
                AppSettings::update(cx, |s| s.import_profile(profile));
                report(&error, None, cx);
            }
            Err(e) => report(&error, Some(&e), cx),
        });
    })
    .detach();
}
