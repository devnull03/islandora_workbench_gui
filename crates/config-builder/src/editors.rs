//! One control per YAML value shape — the vocabulary of mockup `1c`.
//!
//! Every editor follows the same contract: it renders widgets whose ids come from
//! [`field_id`], and [`ConfigBuilder::read_widgets`] reads those same ids back to rebuild the
//! value. Adding a shape means adding an arm to both.
//!
//! The six list-and-map shapes share [`ConfigBuilder::render_rows`]: they differ only in what
//! sits in a row, not in how rows are added, removed or laid out.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable, StyledExt, button::ButtonVariants, checkbox::Checkbox, h_flex,
    input::Input, label::Label, select::Select, v_flex,
};
use serde_yaml::{Mapping, Value};
use workbench_integration::config::{
    catalog::{Browse, SettingDef, Shape},
    validate::Severity,
};

use super::{ConfigBuilder, field_id};

use ui::{APP_CONTROL_SIZE, DetailSelectItem, FieldRow, app_button};

/// What a setting of this shape looks like before anything has been typed into it. Used when a
/// setting has no upstream default, so the row still renders something editable.
pub fn empty_value(shape: Shape) -> Value {
    match shape {
        Shape::Boolean => Value::Bool(false),
        Shape::Integer => Value::Number(0.into()),
        Shape::Map | Shape::MapOfLists => Value::Mapping(Mapping::new()),
        Shape::ListOfStrings
        | Shape::ListOfNumbers
        | Shape::ListOfOneKeyMaps
        | Shape::CommandList
        | Shape::ConfigRef => Value::Sequence(Vec::new()),
        Shape::NullableEnum => Value::Null,
        _ => Value::String(String::new()),
    }
}

/// The rows a list/map shape currently holds, as `(left, right)` string pairs. Normalising the
/// six container shapes to one intermediate is what lets them share a renderer.
fn rows_of(shape: Shape, value: &Value) -> Vec<(String, Vec<String>)> {
    match shape {
        Shape::ListOfStrings | Shape::ListOfNumbers | Shape::CommandList | Shape::ConfigRef => {
            match value {
                Value::Sequence(items) => {
                    items.iter().map(|v| (scalar_text(v), Vec::new())).collect()
                }
                Value::String(one) => vec![(one.clone(), Vec::new())],
                _ => Vec::new(),
            }
        }
        Shape::ListOfOneKeyMaps => match value {
            Value::Sequence(items) => items
                .iter()
                .filter_map(|v| v.as_mapping().and_then(|m| m.iter().next()))
                .map(|(k, v)| (scalar_text(k), vec![scalar_text(v)]))
                .collect(),
            _ => Vec::new(),
        },
        Shape::Map => match value {
            Value::Mapping(m) => m
                .iter()
                .map(|(k, v)| (scalar_text(k), vec![scalar_text(v)]))
                .collect(),
            _ => Vec::new(),
        },
        Shape::MapOfLists => match value {
            Value::Mapping(m) => m
                .iter()
                .map(|(k, v)| (scalar_text(k), string_list(v)))
                .collect(),
            // Workbench's own defaults use a list of one-key mappings for this shape.
            Value::Sequence(items) => items
                .iter()
                .filter_map(|v| v.as_mapping().and_then(|m| m.iter().next()))
                .map(|(k, v)| (scalar_text(k), string_list(v)))
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn string_list(value: &Value) -> Vec<String> {
    match value {
        Value::Sequence(items) => items.iter().map(scalar_text).collect(),
        other => vec![scalar_text(other)],
    }
}

/// A scalar as the text a user would type. Deliberately not `to_string()` on a `Value`, which
/// would quote strings and add a `---` document marker.
fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

/// Parse a typed cell back into YAML. Numbers stay numbers so `500` does not become `"500"`.
fn parse_scalar(text: &str, numeric: bool) -> Value {
    if numeric {
        return match text.trim().parse::<i64>() {
            Ok(n) => Value::Number(n.into()),
            Err(_) => Value::String(text.to_string()),
        };
    }
    Value::String(text.to_string())
}

impl ConfigBuilder {
    // --- reading widgets back into the draft ------------------------------------------

    /// Rebuild one setting's value from its widgets. `None` means "leave the draft alone" —
    /// used by shapes whose control mutates the draft directly (checkbox, row add/remove).
    pub(super) fn read_widgets(&self, def: &SettingDef, cx: &App) -> Option<Value> {
        let key = def.key.as_str();
        let current = self.draft.values.get(key)?;

        match def.shape {
            Shape::Boolean => None, // the checkbox writes straight through
            Shape::Integer => {
                let text = self.input_value(&field_id(&[key]), cx);
                Some(parse_scalar(&text, true))
            }
            Shape::Enum | Shape::NullableEnum => match self.select_value(&field_id(&[key]), cx) {
                Some(v) if v.is_empty() => Some(Value::Null),
                Some(v) => Some(Value::String(v.to_string())),
                None => Some(Value::Null),
            },
            Shape::String
            | Shape::Delimiter
            | Shape::FilePath
            | Shape::Url
            | Shape::TemplateString => Some(Value::String(self.input_value(&field_id(&[key]), cx))),
            Shape::ListOfStrings | Shape::CommandList | Shape::ConfigRef => {
                let n = rows_of(def.shape, current).len();
                Some(Value::Sequence(
                    (0..n)
                        .map(|i| {
                            Value::String(self.input_value(&field_id(&[key, &i.to_string()]), cx))
                        })
                        .collect(),
                ))
            }
            Shape::ListOfNumbers => {
                let n = rows_of(def.shape, current).len();
                Some(Value::Sequence(
                    (0..n)
                        .map(|i| {
                            parse_scalar(
                                &self.input_value(&field_id(&[key, &i.to_string()]), cx),
                                true,
                            )
                        })
                        .collect(),
                ))
            }
            Shape::ListOfOneKeyMaps => {
                let n = rows_of(def.shape, current).len();
                Some(Value::Sequence(
                    (0..n)
                        .map(|i| {
                            let i = i.to_string();
                            let mut m = Mapping::new();
                            m.insert(
                                Value::String(self.input_value(&field_id(&[key, &i, "k"]), cx)),
                                Value::String(self.input_value(&field_id(&[key, &i, "v"]), cx)),
                            );
                            Value::Mapping(m)
                        })
                        .collect(),
                ))
            }
            Shape::Map => {
                let n = rows_of(def.shape, current).len();
                let mut m = Mapping::new();
                for i in 0..n {
                    let i = i.to_string();
                    m.insert(
                        Value::String(self.input_value(&field_id(&[key, &i, "k"]), cx)),
                        Value::String(self.input_value(&field_id(&[key, &i, "v"]), cx)),
                    );
                }
                Some(Value::Mapping(m))
            }
            Shape::MapOfLists => {
                let rows = rows_of(def.shape, current);
                let mut m = Mapping::new();
                for (i, (_, items)) in rows.iter().enumerate() {
                    let i = i.to_string();
                    let values = (0..items.len())
                        .map(|j| {
                            Value::String(
                                self.input_value(&field_id(&[key, &i, "l", &j.to_string()]), cx),
                            )
                        })
                        .collect();
                    m.insert(
                        Value::String(self.input_value(&field_id(&[key, &i, "k"]), cx)),
                        Value::Sequence(values),
                    );
                }
                Some(Value::Mapping(m))
            }
        }
    }

    // --- the settings list -----------------------------------------------------------

    pub(super) fn render_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        // `secondary_tasks` has its own section at the bottom (mockup `1d`), and the locked
        // settings are shown as a band rather than as editable rows.
        let keys: Vec<String> = self
            .draft
            .values
            .keys()
            .filter(|k| k.as_str() != "secondary_tasks")
            .filter(|k| !workbench_integration::config::ConfigDraft::is_app_supplied(k))
            .cloned()
            .collect();

        if keys.is_empty() {
            return vec![
                Label::new("Add task to get started.")
                    .text_color(cx.theme().muted_foreground)
                    .into_any_element(),
            ];
        }
        keys.into_iter()
            .map(|key| self.render_setting(&key, window, cx))
            .collect()
    }

    fn render_setting(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(def) = workbench_integration::config::catalog::find(key) else {
            // An unrecognised key still has to be visible and removable, or a typo in a hand
            // written config becomes invisible the moment it is opened here.
            return self.render_unknown(key, cx).into_any_element();
        };
        let owned_key = key.to_string();
        let control = self.render_control(def, window, cx);
        let notes: Vec<AnyElement> = self
            .problems_for(key)
            .map(|p| {
                let color = match p.severity {
                    Severity::Error => cx.theme().colors.danger,
                    Severity::Warn => cx.theme().colors.warning,
                    Severity::Ok => cx.theme().colors.success,
                };
                Label::new(p.message.clone())
                    .text_xs()
                    .text_color(color)
                    .into_any_element()
            })
            .collect();

        v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().colors.border)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(Label::new(def.key.clone()).font_semibold().text_sm())
                    .when(def.required, |this| {
                        this.child(
                            Label::new("required")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Label::new(def.shape.label())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .when(!def.required, |this| {
                        this.child(
                            app_button(SharedString::from(format!("remove-{}", def.key)))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Close)
                                .tooltip("Remove this setting")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_setting(&owned_key, cx);
                                })),
                        )
                    }),
            )
            .when(!def.description.is_empty(), |this| {
                this.child(
                    Label::new(def.description.clone())
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                )
            })
            .child(control)
            .children(notes)
            .into_any_element()
    }

    fn render_unknown(&mut self, key: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let owned = key.to_string();
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .p_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().colors.warning)
            .child(Label::new(key.to_string()).font_semibold().text_sm())
            .child(
                Label::new("Not a setting Workbench recognises.")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(div().flex_1())
            .child(
                app_button(SharedString::from(format!("remove-unknown-{key}")))
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .on_click(cx.listener(move |this, _, _, cx| this.remove_setting(&owned, cx))),
            )
    }

    // --- one control per shape --------------------------------------------------------

    fn render_control(
        &mut self,
        def: &'static SettingDef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = def.key.clone();
        let value = self.draft.values.get(&key).cloned().unwrap_or(Value::Null);
        let id = field_id(&[&key]);

        match def.shape {
            Shape::Boolean => {
                let checked = value.as_bool().unwrap_or(false);
                let owned = key.clone();
                Checkbox::new(SharedString::from(format!("cb-{key}")))
                    .checked(checked)
                    .label(if checked { "true" } else { "false" })
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        this.draft
                            .values
                            .insert(owned.clone(), Value::Bool(*checked));
                        this.revalidate(cx);
                    }))
                    .into_any_element()
            }

            Shape::Integer => {
                let input = self.input(id, &key, &scalar_text(&value), "0", window, cx);
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .w(px(120.))
                            .child(Input::new(&input).with_size(APP_CONTROL_SIZE)),
                    )
                    .children(def.unit.clone().map(|u| {
                        Label::new(u)
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                    }))
                    .into_any_element()
            }

            Shape::Enum | Shape::NullableEnum => {
                let nullable = def.shape == Shape::NullableEnum;
                let mut items: Vec<DetailSelectItem> = Vec::new();
                if nullable {
                    items.push(DetailSelectItem {
                        label: "not set".into(),
                        subtitle: SharedString::default(),
                        value: SharedString::default(),
                        divider_above: false,
                    });
                }
                items.extend(
                    def.choices
                        .iter()
                        .enumerate()
                        .map(|(i, c)| DetailSelectItem {
                            label: c.label.clone().into(),
                            subtitle: SharedString::default(),
                            value: c.value.clone().into(),
                            divider_above: nullable && i == 0,
                        }),
                );
                let selected = value.as_str().map(|s| SharedString::from(s.to_string()));
                let state = self.select(id, &key, items, selected, window, cx);
                Select::new(&state)
                    .placeholder("Choose…")
                    .with_size(APP_CONTROL_SIZE)
                    .w_full()
                    .into_any_element()
            }

            Shape::FilePath => {
                let is_dir = def.browse == Browse::Dir;
                let prompt: SharedString = if is_dir {
                    format!("Select folder for {key}").into()
                } else {
                    format!("Select file for {key}").into()
                };
                let input = self.input(id, &key, &scalar_text(&value), "", window, cx);
                let browse_input = input.clone();
                FieldRow::new(Input::new(&input).with_size(APP_CONTROL_SIZE).w_full())
                    .child(
                        app_button(SharedString::from(format!("browse-{key}")))
                            .outline()
                            .icon(IconName::FolderOpen)
                            .label("Browse…")
                            // The picker writes into the input asynchronously and the input's
                            // own Change event is what commits, so there is nothing to do here.
                            .on_click(cx.listener(move |_: &mut Self, _, window, cx| {
                                ui::pick_into(window, cx, &browse_input, prompt.clone(), is_dir);
                            })),
                    )
                    .into_any_element()
            }

            Shape::String | Shape::Delimiter | Shape::Url => {
                let placeholder = if def.shape == Shape::Delimiter {
                    "|"
                } else {
                    ""
                };
                let input = self.input(id, &key, &scalar_text(&value), placeholder, window, cx);
                Input::new(&input)
                    .with_size(APP_CONTROL_SIZE)
                    .w_full()
                    .into_any_element()
            }

            Shape::TemplateString => {
                let input = self.input(id, &key, &scalar_text(&value), "", window, cx);
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(Input::new(&input).with_size(APP_CONTROL_SIZE).w_full())
                    .when(!def.tokens.is_empty(), |this| {
                        this.child(
                            Label::new(format!("Insert: {}", def.tokens.join("  ")))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        )
                    })
                    .into_any_element()
            }

            Shape::ListOfStrings
            | Shape::ListOfNumbers
            | Shape::CommandList
            | Shape::ConfigRef
            | Shape::ListOfOneKeyMaps
            | Shape::Map
            | Shape::MapOfLists => self.render_rows(def, &value, window, cx),
        }
    }

    /// The shared editor behind every list and map shape: numbered rows, a remove button per
    /// row, one Add button. Only the cells inside a row differ between shapes.
    fn render_rows(
        &mut self,
        def: &'static SettingDef,
        value: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = def.key.clone();
        let shape = def.shape;
        let rows = rows_of(shape, value);
        let numeric = shape == Shape::ListOfNumbers;
        let numbered = shape == Shape::CommandList;

        let mut list = v_flex().w_full().gap_1();
        for (i, (left, items)) in rows.iter().enumerate() {
            let idx = i.to_string();
            let mut row = h_flex().w_full().gap_2().items_center();

            if numbered {
                row = row.child(
                    Label::new(format!("{}", i + 1))
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                );
            }

            match shape {
                Shape::ListOfStrings
                | Shape::ListOfNumbers
                | Shape::CommandList
                | Shape::ConfigRef => {
                    let input = self.input(field_id(&[&key, &idx]), &key, left, "", window, cx);
                    row = row.child(
                        div()
                            .flex_1()
                            .child(Input::new(&input).with_size(APP_CONTROL_SIZE)),
                    );
                }
                Shape::ListOfOneKeyMaps | Shape::Map => {
                    let k = self.input(field_id(&[&key, &idx, "k"]), &key, left, "key", window, cx);
                    let v = self.input(
                        field_id(&[&key, &idx, "v"]),
                        &key,
                        items.first().map(String::as_str).unwrap_or_default(),
                        "value",
                        window,
                        cx,
                    );
                    row = row
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&k).with_size(APP_CONTROL_SIZE)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&v).with_size(APP_CONTROL_SIZE)),
                        );
                }
                Shape::MapOfLists => {
                    let k = self.input(field_id(&[&key, &idx, "k"]), &key, left, "key", window, cx);
                    let mut cells = h_flex().flex_1().gap_1().flex_wrap();
                    for (j, item) in items.iter().enumerate() {
                        let cell = self.input(
                            field_id(&[&key, &idx, "l", &j.to_string()]),
                            &key,
                            item,
                            "",
                            window,
                            cx,
                        );
                        cells = cells.child(
                            div()
                                .w(px(90.))
                                .child(Input::new(&cell).with_size(APP_CONTROL_SIZE)),
                        );
                    }
                    let owned = key.clone();
                    let row_ix = i;
                    cells = cells.child(
                        app_button(SharedString::from(format!("add-item-{key}-{i}")))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Plus)
                            .tooltip("Add a value")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.push_inner_item(&owned, row_ix, cx);
                            })),
                    );
                    row = row
                        .child(
                            div()
                                .w(px(140.))
                                .child(Input::new(&k).with_size(APP_CONTROL_SIZE)),
                        )
                        .child(cells);
                }
                _ => {}
            }

            let owned = key.clone();
            let row_ix = i;
            row = row.child(
                app_button(SharedString::from(format!("remove-row-{key}-{i}")))
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_row(&owned, row_ix, cx);
                    })),
            );
            list = list.child(row);
        }

        let owned = key.clone();
        list.child(
            h_flex()
                .gap_2()
                .child(
                    app_button(SharedString::from(format!("add-row-{key}")))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Plus)
                        .label(
                            if matches!(
                                shape,
                                Shape::Map | Shape::ListOfOneKeyMaps | Shape::MapOfLists
                            ) {
                                "Add row"
                            } else {
                                "Add"
                            },
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.push_row(&owned, cx);
                        })),
                )
                .when(numeric, |this| {
                    this.child(
                        Label::new("Numbers only")
                            .text_xs()
                            .text_color(cx.theme().muted_foreground),
                    )
                }),
        )
        .into_any_element()
    }

    // --- row mutation ------------------------------------------------------------------
    //
    // These edit the draft directly and then forget the setting's widgets, so the rows that
    // survive are rebuilt from the draft instead of keeping values that have shifted up.

    fn push_row(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(def) = workbench_integration::config::catalog::find(key) else {
            return;
        };
        let entry = self.draft.values.entry(key.to_string());
        let current = entry.or_insert_with(|| empty_value(def.shape));
        match def.shape {
            Shape::Map | Shape::MapOfLists => {
                let map = match current {
                    Value::Mapping(m) => m,
                    other => {
                        *other = Value::Mapping(normalise_to_mapping(other));
                        match other {
                            Value::Mapping(m) => m,
                            _ => unreachable!("just assigned a mapping"),
                        }
                    }
                };
                let blank = if def.shape == Shape::MapOfLists {
                    Value::Sequence(vec![Value::String(String::new())])
                } else {
                    Value::String(String::new())
                };
                map.insert(Value::String(String::new()), blank);
            }
            Shape::ListOfOneKeyMaps => {
                let mut m = Mapping::new();
                m.insert(Value::String(String::new()), Value::String(String::new()));
                as_sequence(current).push(Value::Mapping(m));
            }
            Shape::ListOfNumbers => as_sequence(current).push(Value::Number(0.into())),
            _ => as_sequence(current).push(Value::String(String::new())),
        }
        self.forget_widgets(key);
        self.revalidate(cx);
    }

    fn push_inner_item(&mut self, key: &str, row: usize, cx: &mut Context<Self>) {
        if let Some(Value::Mapping(map)) = self.draft.values.get_mut(key)
            && let Some((_, value)) = map.iter_mut().nth(row)
        {
            as_sequence(value).push(Value::String(String::new()));
        }
        self.forget_widgets(key);
        self.revalidate(cx);
    }

    fn remove_row(&mut self, key: &str, row: usize, cx: &mut Context<Self>) {
        match self.draft.values.get_mut(key) {
            Some(Value::Sequence(items)) if row < items.len() => {
                items.remove(row);
            }
            Some(Value::Mapping(map)) => {
                if let Some(k) = map.keys().nth(row).cloned() {
                    map.remove(&k);
                }
            }
            _ => {}
        }
        self.forget_widgets(key);
        self.revalidate(cx);
    }
}

/// Coerce a value into a sequence in place, so a shape change or a hand-edited file cannot
/// wedge the editor.
fn as_sequence(value: &mut Value) -> &mut Vec<Value> {
    if !value.is_sequence() {
        *value = Value::Sequence(Vec::new());
    }
    match value {
        Value::Sequence(items) => items,
        _ => unreachable!("just assigned a sequence"),
    }
}

/// The list-of-one-key-maps form Workbench also accepts for a map, flattened into a mapping.
fn normalise_to_mapping(value: &Value) -> Mapping {
    let mut out = Mapping::new();
    if let Value::Sequence(items) = value {
        for item in items {
            if let Some(m) = item.as_mapping() {
                for (k, v) in m {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
    }
    out
}
