use std::f32::consts::TAU;

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::core::models::{StandardBiomarker, TrendMatrixData};
use crate::store::AppStore;
use crate::ui::{card, chip_button};

const SERIES_COLORS: [u32; 8] = [
    0x0D9488, // Teal
    0x4F46E5, // Indigo
    0xEA580C, // Orange
    0x9333EA, // Purple
    0x2563EB, // Blue
    0x92400E, // Brown
    0xDB2777, // Pink
    0x16A34A, // Green
];

pub fn dashboard_page(store: AppStore) -> impl IntoElement {
    let plot_width = signal(0.0_f32);

    scroll().flex_1().p_4().child(dynamic(move || {
        let matrix = store.trend_matrix.get();
        let selected = store.selected_codes.get();

        if matrix.dates.is_empty() {
            return card()
                .child(
                    text("暂无已保存的化验单记录，请先在「识别核对」页加载并保存。")
                        .text_sm()
                        .text_color(move || theme().muted_foreground),
                )
                .into_element();
        }

        let active_biomarkers: Vec<StandardBiomarker> = matrix
            .biomarkers
            .iter()
            .filter(|b| matrix.series_by_code.contains_key(&b.code))
            .cloned()
            .collect();

        let visible_biomarkers: Vec<(usize, StandardBiomarker)> = active_biomarkers
            .iter()
            .enumerate()
            .filter(|(_, b)| selected.contains(&b.code))
            .map(|(i, b)| (i, b.clone()))
            .collect();

        let has_pra = visible_biomarkers.iter().any(|(_, b)| b.code == "PRA");
        let has_drc = visible_biomarkers.iter().any(|(_, b)| b.code == "DRC");

        let mut root = view().flex_col().gap_3();

        // 1. Trend Chart Card with Filter Chips + PRA/DRC Isolation + Canvas LineChart
        let mut filter_row = view().flex_row().flex_wrap().gap_1();
        for b in &active_biomarkers {
            let code = b.code.clone();
            let code_sel = code.clone();
            let s_sel = store.clone();
            let s_press = store.clone();
            filter_row = filter_row.child(chip_button(
                format!("{} · {}", b.code, b.display_name),
                move || s_sel.selected_codes.with(|set| set.contains(&code_sel)),
                move || s_press.toggle_selected_code(&code),
            ));
        }

        let mut chart_card = card()
            .flex_col()
            .gap_2()
            .child(
                text("多期指标趋势折线图")
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(move || theme().foreground),
            )
            .child(filter_row);

        if has_pra && has_drc {
            chart_card = chart_card.child(
                view()
                    .p_2()
                    .rounded(move || theme().radius)
                    .border_1()
                    .border_color(move || theme().destructive)
                    .bg(move || theme().destructive.opacity(0.08))
                    .child(
                        text("方法学隔离提示: PRA (血浆肾素活性, ng/mL/h) 与 DRC (直接肾素浓度, uIU/mL) 已作为两条独立曲线渲染，严禁合并换算。")
                            .text_xs()
                            .text_color(move || theme().foreground),
                    ),
            );
        }

        if visible_biomarkers.is_empty() {
            chart_card = chart_card.child(
                view().py_6().items_center().child(
                    text("请选择至少一个指标以显示趋势图")
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                ),
            );
        } else {
            let matrix_for_draw = matrix.clone();
            let visible_for_draw = visible_biomarkers.clone();
            chart_card = chart_card.child(
                canvas(move || {
                    draw_trend_chart(plot_width.get(), &matrix_for_draw, &visible_for_draw)
                })
                .h(px(220.0))
                .on_layout(move |frame: &Bounds<Points>| plot_width.set(frame.size.width.0)),
            );

            let mut date_row = view().flex_row().justify_between().px_4();
            for d in &matrix.dates {
                let short = if d.len() >= 10 { &d[5..] } else { d.as_str() };
                date_row = date_row.child(
                    text(short.to_owned())
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                );
            }
            chart_card = chart_card.child(date_row);

            let mut legend_row = view().flex_row().flex_wrap().gap_3();
            for (idx, b) in &visible_biomarkers {
                let hex = SERIES_COLORS[*idx % SERIES_COLORS.len()];
                legend_row = legend_row.child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(view().w(px(10.0)).h(px(10.0)).bg(move || rgb(hex)))
                        .child(
                            text(format!("{} ({})", b.display_name, b.standard_unit))
                                .text_xs()
                                .text_color(move || theme().foreground),
                        ),
                );
            }
            chart_card = chart_card.child(legend_row);
        }

        root = root.child(chart_card);

        // 2. Horizontally Scrollable Multi-Period Pivot Table
        let mut header = view()
            .flex_row()
            .gap_2()
            .py_1()
            .border_b_1()
            .border_color(move || theme().border);
        header = header.child(
            view().w(px(210.0)).child(
                text("指标 (标准单位)")
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(move || theme().muted_foreground),
            ),
        );
        for d in &matrix.dates {
            header = header.child(
                view().w(px(110.0)).child(
                    text(d.clone())
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().muted_foreground),
                ),
            );
        }

        let mut table_rows = view().flex_col().child(header);
        for b in &active_biomarkers {
            let mut row = view()
                .flex_row()
                .gap_2()
                .py_1()
                .border_b_1()
                .border_color(move || theme().border);
            row = row.child(
                view().w(px(210.0)).child(
                    text(format!("{} ({})", b.display_name, b.standard_unit))
                        .text_xs()
                        .text_color(move || theme().foreground),
                ),
            );
            let row_points = matrix.points_by_code_and_date.get(&b.code);
            for d in &matrix.dates {
                let (cell_str, is_high, is_low) = match row_points.and_then(|m| m.get(d)) {
                    Some(pt) => {
                        let flag = match pt.abnormal_flag.as_str() {
                            "HIGH" => " ↑",
                            "LOW" => " ↓",
                            _ => "",
                        };
                        (
                            format!("{:.2}{flag}", pt.normalized_value),
                            pt.abnormal_flag == "HIGH",
                            pt.abnormal_flag == "LOW",
                        )
                    }
                    None => ("-".to_owned(), false, false),
                };
                row = row.child(
                    view().w(px(110.0)).child(
                        text(cell_str)
                            .text_xs()
                            .font_weight(if is_high || is_low {
                                FontWeight::BOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(move || {
                                if is_high {
                                    theme().destructive
                                } else if is_low {
                                    theme().primary
                                } else {
                                    theme().foreground
                                }
                            }),
                    ),
                );
            }
            table_rows = table_rows.child(row);
        }

        let table_card = card()
            .flex_col()
            .gap_2()
            .child(
                text("跨期对比透视表 (已换算至标准单位)")
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(move || theme().foreground),
            )
            .child(scroll().horizontal().child(table_rows));

        root = root.child(table_card);
        root.into_element()
    }))
}

fn draw_trend_chart(
    width: f32,
    matrix: &TrendMatrixData,
    visible: &[(usize, StandardBiomarker)],
) -> Painter {
    let w = width.max(240.0);
    let h = 220.0_f32;
    let pad_l = 16.0_f32;
    let pad_r = 16.0_f32;
    let pad_t = 16.0_f32;
    let pad_b = 16.0_f32;
    let plot_w = (w - pad_l - pad_r).max(1.0);
    let plot_h = (h - pad_t - pad_b).max(1.0);

    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (_, b) in visible {
        if let Some(pts_map) = matrix.points_by_code_and_date.get(&b.code) {
            for pt in pts_map.values() {
                min_y = min_y.min(pt.normalized_value);
                max_y = max_y.max(pt.normalized_value);
            }
        }
    }

    if !min_y.is_finite() || !max_y.is_finite() {
        min_y = 0.0;
        max_y = 10.0;
    } else if (max_y - min_y).abs() < 1e-6 {
        min_y = (min_y * 0.8).max(0.0);
        max_y = (max_y * 1.2).max(min_y + 1.0);
    } else {
        let span = max_y - min_y;
        min_y = (min_y - span * 0.15).max(0.0);
        max_y += span * 0.15;
    }

    let mut p = Painter::new()
        .stroke_color(color(theme().border))
        .line_width(1.0);

    for step in 0..=4 {
        let y = pad_t + plot_h * (step as f32 / 4.0);
        p = p.begin_path().move_to(pad_l, y).line_to(w - pad_r, y).stroke();
    }

    let n_dates = matrix.dates.len();
    for (idx, b) in visible {
        let Some(pts_map) = matrix.points_by_code_and_date.get(&b.code) else {
            continue;
        };
        let mut coords: Vec<(f32, f32)> = Vec::new();
        for (i, d) in matrix.dates.iter().enumerate() {
            if let Some(pt) = pts_map.get(d) {
                let x = if n_dates <= 1 {
                    pad_l + plot_w * 0.5
                } else {
                    pad_l + plot_w * (i as f32 / (n_dates - 1) as f32)
                };
                let norm_y = ((pt.normalized_value - min_y) / (max_y - min_y)) as f32;
                let y = pad_t + plot_h * (1.0 - norm_y.clamp(0.0, 1.0));
                coords.push((x, y));
            }
        }
        if coords.is_empty() {
            continue;
        }

        let hex = SERIES_COLORS[*idx % SERIES_COLORS.len()];
        p = p.stroke_color(color(rgb(hex))).line_width(2.5);
        if coords.len() >= 2 {
            p = p.begin_path().move_to(coords[0].0, coords[0].1);
            for &(x, y) in coords.iter().skip(1) {
                p = p.line_to(x, y);
            }
            p = p.stroke();
        }
        p = p.fill_color(color(rgb(hex)));
        for &(x, y) in &coords {
            p = p.begin_path().arc(x, y, 4.0, 0.0, TAU).fill();
        }
    }

    p
}
