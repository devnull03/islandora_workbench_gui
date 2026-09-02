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
    ActiveTheme, IconName, Selectable as _, Sizable, StyledExt,
    button::{Button, ButtonGroup},
    h_flex,
    input::{Input, NumberInput},
    label::Label,
    switch::Switch,
    v_flex,
};
use serde_yaml::{Mapping, Value};
use workbench_integration::config::{
    catalog::{Browse, SettingDef, Shape},
    validate::Severity,
};

use super::{ConfigBuilder, field_id};

use ui::tokens::{
    CHAR_FIELD_W, GAP_MD, GAP_XS, KEY_COL_W, LIST_CELL_W, NUMBER_FIELD_W, ROW_ACTION_W,
};
use ui::{
    APP_CONTROL_SIZE, Card, CardTone, ChipList, DetailSelectItem, InlineMessage,
    MAX_INLINE_ENUM_OPTIONS, PathField, RowActions, RowEditor, SettingRow, add_row_button,
    app_button, ghost_button,
};

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
            // Chips have no per-item widget to read: committing one and removing one both edit
            // the draft directly, the same way the switch does.
            Shape::ListOfStrings | Shape::ListOfNumbers => None,
            Shape::CommandList | Shape::ConfigRef => {
                let n = rows_of(def.shape, current).len();
                Some(Value::Sequence(
                    (0..n)
                        .map(|i| {
                            Value::String(self.input_value(&field_id(&[key, &i.to_string()]), cx))
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
        // §03: rows are separated by a 1px rule and the last row has none — the section's own
        // edge is already there, and two lines a pixel apart read as a rendering bug.
        let last = keys.len().saturating_sub(1);
        keys.into_iter()
            .enumerate()
            .map(|(i, key)| {
                let row = self.render_setting(&key, window, cx);
                div()
                    .w_full()
                    .when(i != last, |el| {
                        el.border_b_1()
                            .border_color(cx.theme().colors.table_row_border)
                    })
                    .child(row)
                    .into_any_element()
            })
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
        // The row shows the most severe thing wrong with the value. There is only one message
        // slot under the control by design: a stack of five is a wall nobody reads.
        let note = self
            .problems_for(key)
            .filter_map(|p| match p.severity {
                Severity::Error => Some(InlineMessage::error(p.message.clone())),
                Severity::Warn => Some(InlineMessage::warning(p.message.clone())),
                Severity::Ok => None,
            })
            .min_by_key(InlineMessage::level);
        // A boolean is one short control, so its label centres against it rather than sitting
        // at the top of a row that is only 19px tall (§05).
        let align_center = def.shape == Shape::Boolean;
        // §03: the hint only appears once the value has moved off the default. Showing
        // `default: false` beside a `false` is noise on every untouched row.
        let modified = self
            .draft
            .values
            .get(&def.key)
            .is_some_and(|value| value != &def.default);

        SettingRow::new(def.key.clone(), control)
            // The label in the builder is the literal YAML key, so it is set in mono; in the
            // settings window the same component labels prose and is not.
            .mono_label(true)
            .required(def.required)
            .type_badge(def.shape.label())
            .align_center(align_center)
            .modified(modified)
            .when(!def.description.is_empty(), |row| {
                row.description(def.description.clone())
            })
            .note(note)
            .when(!def.required, |row| {
                row.child(
                    ghost_button(SharedString::from(format!("remove-{}", def.key)))
                        .icon(IconName::Close)
                        .tooltip("Remove this setting")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_setting(&owned_key, cx);
                        })),
                )
            })
            .into_any_element()
    }
    fn render_unknown(&mut self, key: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let owned = key.to_string();
        Card::new().tone(CardTone::Warning).child(
            h_flex()
                .w_full()
                .gap_2()
                .items_center()
                .child(Label::new(key.to_string()).font_semibold().text_sm())
                .child(
                    Label::new("Not a setting Workbench recognises.")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(div().flex_1())
                .child(
                    ghost_button(SharedString::from(format!("remove-unknown-{key}")))
                        .icon(IconName::Close)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.remove_setting(&owned, cx)),
                        ),
                ),
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
            // §05: a YAML boolean is always a Switch. The Checkbox is reserved for run-time
            // options like Auto-accept, which are not written to the file.
            Shape::Boolean => {
                let checked = value.as_bool().unwrap_or(false);
                let owned = key.clone();
                let default = def.default.as_bool();
                h_flex()
                    .gap(GAP_MD)
                    .items_center()
                    .child(
                        Switch::new(SharedString::from(format!("sw-{key}")))
                            .checked(checked)
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                this.draft
                                    .values
                                    .insert(owned.clone(), Value::Bool(*checked));
                                this.revalidate(cx);
                            })),
                    )
                    // The literal YAML word, not "On" — this row is a view of a file, and the
                    // file says `true`. Muted when false, so a page of switches reads at a
                    // glance as the set that is on.
                    .child(
                        Label::new(if checked { "true" } else { "false" })
                            .text_sm()
                            .map(|label| {
                                if checked {
                                    label
                                } else {
                                    label.text_color(cx.theme().muted_foreground)
                                }
                            }),
                    )
                    .when_some(default.filter(|d| *d != checked), |this, default| {
                        this.child(
                            Label::new(format!("default: {default}"))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        )
                    })
                    .into_any_element()
            }

            // §04: an integer gets − value + rather than a bare field. The state carries the
            // digits-only pattern and a floor, which is what lets NumberInput step the value
            // itself — no Step event to subscribe to, and no second copy of the number.
            Shape::Integer => {
                let input = self.number_input(id, &key, &scalar_text(&value), window, cx);
                h_flex()
                    .gap(GAP_MD)
                    .items_center()
                    .child(
                        div()
                            .w(NUMBER_FIELD_W)
                            .child(NumberInput::new(&input).with_size(APP_CONTROL_SIZE)),
                    )
                    // The unit sits outside the control: it is not editable, and inside the
                    // frame it reads as part of the value.
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
                    // §05: `none` is a real row, and selecting it omits the key from the
                    // YAML. Mono like the values it sits above, because it stands for one.
                    items.push(DetailSelectItem::code("", "none").subtitle("leave this unset"));
                }
                items.extend(def.choices.iter().enumerate().map(|(i, c)| {
                    DetailSelectItem::from_choice(c.value.clone(), &c.label)
                        .divider_above(nullable && i == 0)
                }));
                let selected = value.as_str().map(|s| SharedString::from(s.to_string()));
                let state = self.select(
                    id.clone(),
                    &key,
                    items.clone(),
                    selected.clone(),
                    window,
                    cx,
                );

                // §05. A nullable enum keeps the dropdown: `none` is a real row in the menu,
                // last and below a rule, and selecting it omits the key from the YAML — there
                // is no separate clear button, and a segment reading "none" beside real values
                // would imply null is one of them.
                if nullable {
                    let choices: Vec<String> =
                        def.choices.iter().map(|c| c.label.clone()).collect();
                    return v_flex()
                        .gap(GAP_XS)
                        .child(ui::app_select(&state).placeholder("Choose…").w_full())
                        .child(
                            Label::new(format!("Choices: not set · {}", choices.join(" · ")))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .into_any_element();
                }

                // Past four options a strip is wider than the dropdown it replaced, so the
                // dropdown wins — `task` with its twelve values included.
                if items.len() > MAX_INLINE_ENUM_OPTIONS {
                    return ui::app_select(&state)
                        .placeholder("Choose…")
                        .w_full()
                        .into_any_element();
                }

                // A short enum reads better as every option at once than as a dropdown hiding
                // three of four. The select state still exists and is still what `read_widgets`
                // reads back — the segments write into it rather than owning a rival answer.
                //
                // Segments show the bare YAML value, never the prose label: `replace` fits a
                // cell, `replace - Replace the field values` does not, and the row is a view of
                // a file that says `replace`.
                let group_id = id.clone();
                ButtonGroup::new(id)
                    .with_size(APP_CONTROL_SIZE)
                    .outline()
                    .children(items.into_iter().map(move |item| {
                        let value = item.value;
                        let is_selected = selected.as_ref() == Some(&value);
                        let write = state.clone();
                        Button::new(SharedString::from(format!("{group_id}-{value}")))
                            .label(value.clone())
                            .selected(is_selected)
                            .toggled(is_selected)
                            // Keep the handler on the child: ButtonGroup's aggregate callback
                            // intentionally does not receive keyboard activation upstream.
                            .on_click(move |_, window, cx| {
                                write.update(cx, |state, cx| {
                                    state.set_selected_value(&value, window, cx);
                                });
                            })
                    }))
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
                PathField::new(SharedString::from(format!("browse-{key}")), &input)
                    .prompt(prompt)
                    .directories(is_dir)
                    .into_any_element()
            }
            // One character, so a full-width field would be a lie about what fits. 44px is the
            // design language`s char field: wide enough to see the glyph, narrow enough that
            // nobody types a word into it.
            Shape::Delimiter => {
                let input = self.input(id, &key, &scalar_text(&value), "|", window, cx);
                div()
                    .w(CHAR_FIELD_W)
                    .child(Input::new(&input).with_size(APP_CONTROL_SIZE).w_full())
                    .into_any_element()
            }

            Shape::String | Shape::Url => {
                let input = self.input(id, &key, &scalar_text(&value), "", window, cx);
                Input::new(&input).w_full().into_any_element()
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

            // §05: chips imply order does not matter, so only the two set-shaped lists get
            // them. `csv_field_templates` and the command hooks stay rows, because their order
            // is part of the value.
            Shape::ListOfStrings | Shape::ListOfNumbers => {
                self.render_chips(def, &value, window, cx)
            }

            Shape::CommandList
            | Shape::ConfigRef
            | Shape::ListOfOneKeyMaps
            | Shape::Map
            | Shape::MapOfLists => self.render_rows(def, &value, window, cx),
        }
    }

    /// A set as chips, with a dashed slot on the end to add another (§05).
    ///
    /// The add slot is an ordinary cached input, so it survives a re-render; what is unusual is
    /// that its subscription commits on Enter and on blur rather than on every keystroke. A
    /// per-keystroke commit would append a chip per character.
    fn render_chips(
        &mut self,
        def: &'static SettingDef,
        value: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = def.key.clone();
        let chips: Vec<SharedString> = rows_of(def.shape, value)
            .into_iter()
            .map(|(left, _)| SharedString::from(left))
            .collect();
        let add = self.chip_input(field_id(&[&key, "add"]), &key, window, cx);
        let owned = key.clone();
        // Not `cx.listener`: that hands the callback its event by reference, and a chip index is
        // a plain `usize`. A weak handle is the same amount of code and says what it does.
        let this = cx.entity().downgrade();

        v_flex()
            .w_full()
            .gap(GAP_XS)
            .child(ChipList::new(key.clone(), chips).add_slot(&add).on_remove(
                move |index, _, cx| {
                    let Some(builder) = this.upgrade() else {
                        return;
                    };
                    builder.update(cx, |builder, cx| builder.remove_row(&owned, index, cx));
                },
            ))
            .when(def.shape == Shape::ListOfNumbers, |this| {
                this.child(
                    Label::new("Numbers only")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                )
            })
            .into_any_element()
    }

    /// Append `text` to a chip set, unless it is blank or already there.
    ///
    /// §05: duplicates are dropped silently rather than flagged. Re-typing a value you already
    /// have is not an error worth a message — the set already says what you meant.
    pub(super) fn push_chip(&mut self, key: &str, text: &str, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some(def) = workbench_integration::config::catalog::find(key) else {
            return;
        };
        let numeric = def.shape == Shape::ListOfNumbers;
        let chip = parse_scalar(text, numeric);
        let entry = self.draft.values.entry(key.to_string());
        let current = entry.or_insert_with(|| empty_value(def.shape));
        let items = as_sequence(current);
        if !items.contains(&chip) {
            items.push(chip);
        }
        self.revalidate(cx);
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

        // §07 shows the header band only when the columns are named, which is the keyed shapes
        // and not a plain list — a header over one unnamed column names nothing.
        //
        // The titles are generic because the catalogue has none: `scripts/gen-config-catalog.py`
        // would have to grow a `columns` field before this could say "Media type" / "Drupal
        // field" the way the mockup does. Generic beats invented.
        let columns: Option<(&str, &str)> = match shape {
            Shape::Map => Some(("Key", "Value")),
            Shape::ListOfOneKeyMaps => Some(("Name", "Template")),
            Shape::MapOfLists => Some(("Key", "Values")),
            _ => None,
        };

        let mut list = RowEditor::new();
        if let Some((left, right)) = columns {
            let muted = cx.theme().colors.table_head_foreground;
            let mono = cx.theme().mono_font_family.clone();
            list = list.header(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        div().flex_1().min_w(px(0.)).child(
                            Label::new(left.to_uppercase())
                                .text_xs()
                                .font_family(mono.clone())
                                .text_color(muted),
                        ),
                    )
                    .child(
                        div().flex_1().min_w(px(0.)).child(
                            Label::new(right.to_uppercase())
                                .text_xs()
                                .font_family(mono)
                                .text_color(muted),
                        ),
                    )
                    .child(div().w(ROW_ACTION_W).flex_none()),
            );
        }
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
                                .w(LIST_CELL_W)
                                .child(Input::new(&cell).with_size(APP_CONTROL_SIZE)),
                        );
                    }
                    let owned = key.clone();
                    let row_ix = i;
                    cells = cells.child(
                        app_button(SharedString::from(format!("add-item-{key}-{i}")))
                            .icon(IconName::Plus)
                            .tooltip("Add a value")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.push_inner_item(&owned, row_ix, cx);
                            })),
                    );
                    row = row
                        .child(
                            div()
                                .w(KEY_COL_W)
                                .child(Input::new(&k).with_size(APP_CONTROL_SIZE)),
                        )
                        .child(cells);
                }
                _ => {}
            }

            let owned = key.clone();
            let row_ix = i;
            row = row.child(
                RowActions::new().child(
                    ghost_button(SharedString::from(format!("remove-row-{key}-{i}")))
                        .icon(IconName::Close)
                        .tooltip("Remove this row")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_row(&owned, row_ix, cx);
                        })),
                ),
            );
            list = list.child(row);
        }

        let owned = key.clone();
        list.add_row(
            h_flex()
                .gap_2()
                .child(
                    add_row_button(
                        SharedString::from(format!("add-row-{key}")),
                        if matches!(
                            shape,
                            Shape::Map | Shape::ListOfOneKeyMaps | Shape::MapOfLists
                        ) {
                            "row"
                        } else {
                            "entry"
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

    pub(super) fn remove_row(&mut self, key: &str, row: usize, cx: &mut Context<Self>) {
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
