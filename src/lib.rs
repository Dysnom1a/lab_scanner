pub mod core;
pub mod pages;
pub mod shell;
pub mod store;
pub mod ui;

use nsh_base::theme;
use nsh_core::prelude::*;

#[native_module("lab-scanner/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

#[component]
pub fn App() -> impl IntoElement {
    let store = store::AppStore::open_default();
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .p_safe_area()
        .child(shell::shell(store))
}
