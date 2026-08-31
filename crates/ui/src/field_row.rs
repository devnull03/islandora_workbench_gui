//! One control on a line with the buttons that act on it.
//!
//! `flex_1().min_w(px(0.))` on the leading child is not decoration: without the zero min-width a
//! flex item refuses to shrink below its content, so a long path pushes the Browse button off the
//! row instead of ellipsising. That pairing was written out by hand at seven call sites, which is
//! seven chances to forget the `min_w`.

use gpui::*;
use gpui_component::h_flex;

use crate::tokens::GAP_MD;

#[derive(IntoElement)]
pub struct FieldRow {
    lead: AnyElement,
    trailing: Vec<AnyElement>,
}

impl FieldRow {
    /// `lead` is the control that flexes — an input, a select, a path field.
    pub fn new(lead: impl IntoElement) -> Self {
        Self {
            lead: lead.into_any_element(),
            trailing: Vec::new(),
        }
    }
}

/// Children are the trailing actions; the flexing control is [`FieldRow::new`]'s argument.
impl ParentElement for FieldRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.trailing.extend(elements);
    }
}

impl RenderOnce for FieldRow {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .items_center()
            .gap(GAP_MD)
            .child(div().flex_1().min_w(px(0.)).child(self.lead))
            .children(self.trailing)
    }
}
