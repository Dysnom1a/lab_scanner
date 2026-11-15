use nsh_base::theme;
use nsh_core::prelude::*;

use crate::pages::{dashboard::dashboard_page, review::review_page};
use crate::store::{AppStore, Tab};
use crate::ui::chip_button;

pub fn shell(store: AppStore) -> impl IntoElement {
    let s_tab1 = store.clone();
    let s_tab2 = store.clone();
    let s_body = store.clone();

    view()
        .flex_1()
        .flex_col()
        .bg(move || theme().background)
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(move || theme().border)
                .child(
                    text("离线医疗化验单解析引擎 (NSH Linux)")
                        .text_base()
                        .font_weight(FontWeight::BOLD)
                        .text_color(move || theme().foreground),
                )
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .child(chip_button(
                            "识别核对",
                            move || s_tab1.active_tab.get() == Tab::Review,
                            {
                                let s = store.clone();
                                move || s.active_tab.set(Tab::Review)
                            },
                        ))
                        .child(chip_button(
                            "趋势看板",
                            move || s_tab2.active_tab.get() == Tab::Dashboard,
                            {
                                let s = store.clone();
                                move || s.active_tab.set(Tab::Dashboard)
                            },
                        )),
                ),
        )
        .child(
            view()
                .px_4()
                .py_2()
                .bg(move || theme().muted)
                .border_b_1()
                .border_color(move || theme().border)
                .child(
                    text(move || store.status_message.get())
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            when(
                move || s_body.active_tab.get() == Tab::Review,
                {
                    let s = store.clone();
                    move || review_page(s.clone())
                },
            )
            .otherwise({
                let s = store.clone();
                move || dashboard_page(s.clone())
            }),
        )
}
