use nsh_base::{button, input, theme};
use nsh_core::prelude::*;

/// Primary solid button using semantic theme tokens.
pub fn primary_button(
    label: impl IntoReactive<SharedString>,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    button()
        .on_press(on_press)
        .h_9()
        .px_4()
        .items_center()
        .justify_center()
        .rounded(move || theme().radius)
        .bg(move || theme().primary)
        .active(|s| s.opacity(0.85))
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .child(
            text(label)
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().primary_foreground),
        )
}

/// Secondary / chip button with active-selected state.
pub fn chip_button(
    label: impl IntoReactive<SharedString>,
    selected: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    button()
        .on_press(on_press)
        .selected(selected)
        .h_8()
        .px_3()
        .items_center()
        .justify_center()
        .rounded(move || theme().radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().secondary)
        .selected_style(|s| {
            s.bg(move || theme().primary)
                .border_color(move || theme().primary)
        })
        .active(|s| s.opacity(0.85))
        .child(
            text(label)
                .text_xs()
                .text_color(move || theme().secondary_foreground),
        )
}

/// Styled text field bound to a reactive value.
pub fn text_field(
    placeholder: &'static str,
    value: impl Fn() -> String + 'static,
    on_change: impl Fn(&str) + 'static,
) -> impl IntoElement {
    input()
        .value(value)
        .on_change(on_change)
        .placeholder(placeholder)
        .placeholder_color(|| color(theme().muted_foreground))
        .caret_color(|| color(theme().primary))
        .h_9()
        .px_3()
        .text_sm()
        .text_color(move || theme().foreground)
        .bg(move || theme().background)
        .rounded(move || theme().radius)
        .border_1()
        .border_color(move || theme().input)
        .focus(|s| s.border_color(move || theme().ring))
}

/// Card container with semantic border & background.
pub fn card() -> View {
    view()
        .p_3()
        .gap_2()
        .rounded(move || theme().radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
}
