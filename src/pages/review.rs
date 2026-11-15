use nsh_base::{button, theme};
use nsh_core::prelude::*;

use crate::core::models::ParsedLabItem;
use crate::store::AppStore;
use crate::ui::{card, chip_button, primary_button, text_field};

pub fn review_page(store: AppStore) -> impl IntoElement {
    let store_demo = store.clone();
    let store_pick = store.clone();
    let store_file = store.clone();
    let store_save = store.clone();
    let store_list = store.clone();

    scroll()
        .flex_1()
        .p_4()
        .child(
            view()
                .flex_col()
                .gap_3()
                .child(
                    card()
                        .flex_col()
                        .child(
                            text("1. 导入 / 模拟 OCR 识别")
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(primary_button("选择本地图片", move || {
                                    store_pick.pick_and_recognize_local_file();
                                }))
                                .child(primary_button("加载模拟化验单", move || {
                                    store_demo.load_demo();
                                }))
                                .child(
                                    view().flex_1().child(text_field(
                                        "本地图片或 OCR JSON 路径",
                                        {
                                            let s = store.clone();
                                            move || s.ocr_json_path.get()
                                        },
                                        {
                                            let s = store.clone();
                                            move |v| s.ocr_json_path.set(v.to_owned())
                                        },
                                    )),
                                )
                                .child(primary_button("识别/解析", move || {
                                    store_file.load_local_file();
                                })),
                        ),
                )
                .child(
                    card()
                        .flex_col()
                        .child(
                            text("2. 检查单元数据")
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            view()
                                .flex_row()
                                .gap_2()
                                .child(
                                    view().flex_1().child(text_field(
                                        "检查日期 YYYY-MM-DD",
                                        {
                                            let s = store.clone();
                                            move || s.test_date.get()
                                        },
                                        {
                                            let s = store.clone();
                                            move |v| s.test_date.set(v.to_owned())
                                        },
                                    )),
                                )
                                .child(
                                    view().flex_1().child(text_field(
                                        "医院名称 (可选)",
                                        {
                                            let s = store.clone();
                                            move || s.hospital.get()
                                        },
                                        {
                                            let s = store.clone();
                                            move |v| s.hospital.set(v.to_owned())
                                        },
                                    )),
                                )
                                .child(
                                    view()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .child(chip_button(
                                            "未知",
                                            {
                                                let s = store.clone();
                                                move || s.posture.get() == "UNKNOWN"
                                            },
                                            {
                                                let s = store.clone();
                                                move || s.posture.set("UNKNOWN".to_owned())
                                            },
                                        ))
                                        .child(chip_button(
                                            "立位",
                                            {
                                                let s = store.clone();
                                                move || s.posture.get() == "UPRIGHT"
                                            },
                                            {
                                                let s = store.clone();
                                                move || s.posture.set("UPRIGHT".to_owned())
                                            },
                                        ))
                                        .child(chip_button(
                                            "卧位",
                                            {
                                                let s = store.clone();
                                                move || s.posture.get() == "SUPINE"
                                            },
                                            {
                                                let s = store.clone();
                                                move || s.posture.set("SUPINE".to_owned())
                                            },
                                        )),
                                ),
                        ),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            text(move || {
                                let count = store.items.with(|l| l.len());
                                format!("3. 解析结果核对 ({count} 项)")
                            })
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().foreground),
                        )
                        .child(primary_button("确认保存", move || {
                            store_save.save_current_record();
                        })),
                )
                .child(each(
                    store_list.items,
                    |item: &ParsedLabItem| item.result_id.clone(),
                    move |item, index| {
                        let store_row = store_list.clone();
                        let result_id = item.with_untracked(|it| it.result_id.clone());
                        let is_high_conf = computed(move || {
                            item.with(|it| it.confidence >= 0.85 && it.biomarker_code.is_some())
                        });
                        let is_unclassified =
                            computed(move || item.with(|it| it.biomarker_code.is_none()));
                        let is_expanded = {
                            let s = store_row.clone();
                            let id = result_id.clone();
                            computed(move || s.expanded_ids.with(|set| set.contains(&id)))
                        };
                        let candidates = item.with_untracked(|it| it.top_candidates.clone());

                        card()
                            .p_0()
                            .border_l(px(5.0))
                            .border_color(move || {
                                if is_high_conf.get() {
                                    theme().primary
                                } else {
                                    theme().destructive
                                }
                            })
                            .bg(move || {
                                if is_high_conf.get() {
                                    theme().card
                                } else if is_unclassified.get() {
                                    theme().destructive.opacity(0.08)
                                } else {
                                    theme().accent
                                }
                            })
                            .child(
                                button()
                                    .on_press({
                                        let s = store_row.clone();
                                        let id = result_id.clone();
                                        move || s.toggle_expanded(&id)
                                    })
                                    .px_3()
                                    .py_2()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .active(|s| s.opacity(0.85))
                                    .child(
                                        text(move || {
                                            item.with(|it| {
                                                let label = it
                                                    .display_name
                                                    .as_deref()
                                                    .unwrap_or(&it.raw_name);
                                                let op = if it.operator_symbol == "=" {
                                                    ""
                                                } else {
                                                    it.operator_symbol.as_str()
                                                };
                                                let unit = it.raw_unit.as_deref().unwrap_or("");
                                                format!("{label} : {op}{} {unit}", it.raw_value)
                                            })
                                        })
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(move || {
                                            if is_unclassified.get() {
                                                theme().destructive
                                            } else {
                                                theme().foreground
                                            }
                                        }),
                                    )
                                    .child(
                                        view()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                text(move || {
                                                    item.with(|it| match it.abnormal_flag.as_str()
                                                    {
                                                        "HIGH" => "↑ 高",
                                                        "LOW" => "↓ 低",
                                                        _ => "",
                                                    })
                                                })
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(move || theme().destructive),
                                            )
                                            .child(
                                                text(move || {
                                                    item.with(|it| {
                                                        format!(
                                                            "{}%",
                                                            (it.confidence * 100.0) as i32
                                                        )
                                                    })
                                                })
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(move || {
                                                    if is_high_conf.get() {
                                                        theme().primary
                                                    } else {
                                                        theme().destructive
                                                    }
                                                }),
                                            )
                                            .child(
                                                text(move || {
                                                    if is_expanded.get() {
                                                        "▲"
                                                    } else {
                                                        "▼"
                                                    }
                                                })
                                                .text_xs()
                                                .text_color(move || theme().muted_foreground),
                                            ),
                                    ),
                            )
                            .child(when(is_expanded, move || {
                                let store_body = store_row.clone();
                                let cands = candidates.clone();
                                view()
                                    .px_3()
                                    .pb_3()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        text(move || {
                                            item.with(|it| {
                                                let ref_info = it
                                                    .ref_range_raw
                                                    .as_deref()
                                                    .map(|r| format!("  |  参考区间: {r}"))
                                                    .unwrap_or_default();
                                                format!(
                                                    "OCR 原始名称: {}{}  |  BBox: [{:.0},{:.0},{:.0},{:.0}]",
                                                    it.raw_name,
                                                    ref_info,
                                                    it.row_bbox[0],
                                                    it.row_bbox[1],
                                                    it.row_bbox[2],
                                                    it.row_bbox[3]
                                                )
                                            })
                                        })
                                        .text_xs()
                                        .text_color(move || theme().muted_foreground),
                                    )
                                    .child(
                                        view()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                view().w(px(120.0)).child(text_field(
                                                    "数值",
                                                    move || {
                                                        item.with(|it| it.raw_value.to_string())
                                                    },
                                                    {
                                                        let s = store_body.clone();
                                                        move |v| {
                                                            s.update_item_value(
                                                                index.get_untracked(),
                                                                v,
                                                            )
                                                        }
                                                    },
                                                )),
                                            )
                                            .child(
                                                view().w(px(120.0)).child(text_field(
                                                    "单位",
                                                    move || {
                                                        item.with(|it| {
                                                            it.raw_unit.clone().unwrap_or_default()
                                                        })
                                                    },
                                                    {
                                                        let s = store_body.clone();
                                                        move |v| {
                                                            s.update_item_unit(
                                                                index.get_untracked(),
                                                                v,
                                                            )
                                                        }
                                                    },
                                                )),
                                            )
                                            .child(
                                                text(move || {
                                                    item.with(|it| {
                                                        it.normalized_value
                                                            .zip(it.standard_unit.as_deref())
                                                            .map(|(v, u)| {
                                                                format!("标准换算值: {v:.3} {u}")
                                                            })
                                                            .unwrap_or_default()
                                                    })
                                                })
                                                .text_xs()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(move || theme().primary),
                                            ),
                                    )
                                    .when(!cands.is_empty(), |parent| {
                                        let mut row_chips = view().flex_row().flex_wrap().gap_1();
                                        for cand in cands {
                                            let code = cand.code.clone();
                                            let label = format!(
                                                "{} ({}%)",
                                                cand.display_name,
                                                (cand.score * 100.0) as i32
                                            );
                                            let s = store_body.clone();
                                            let code_sel = code.clone();
                                            row_chips = row_chips.child(chip_button(
                                                label,
                                                move || {
                                                    item.with(|it| {
                                                        it.biomarker_code.as_deref()
                                                            == Some(&code_sel)
                                                    })
                                                },
                                                move || {
                                                    s.bind_item_biomarker(
                                                        index.get_untracked(),
                                                        &code,
                                                    );
                                                },
                                            ));
                                        }
                                        parent.child(row_chips)
                                    })
                            }))
                    },
                )),
        )
}
