use nsh_app_lab_scanner::core::database::AppDatabase;
use nsh_app_lab_scanner::core::normalizer::{clean_biomarker_name, combined_text_score, normalize_unit};
use nsh_app_lab_scanner::core::pipeline::{demo_ocr_response, parse_ocr_response};
use nsh_app_lab_scanner::shell::shell;
use nsh_app_lab_scanner::store::AppStore;
use nsh_base::theme;
use nsh_core::prelude::*;
use nsh_engine::{EngineConfig, Points, Root, Size};
use nsh_testing::Headless;

#[component]
fn TestApp() -> impl IntoElement {
    let store = AppStore::open_in_memory();
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(shell(store))
}

#[test]
fn normalizer_and_pipeline_work_end_to_end() {
    assert_eq!(
        clean_biomarker_name("  血浆醛固酮（立位）*# "),
        "血浆醛固酮立位"
    );
    assert_eq!(normalize_unit(Some("mlU/L")).as_deref(), Some("mIU/L"));
    assert_eq!(normalize_unit(Some("umol/l")).as_deref(), Some("umol/L"));
    assert!(combined_text_score("血浆醛固酮", "血浆醛固酮浓度") > 0.7);

    let db = AppDatabase::open_in_memory().unwrap();
    let dict = db.load_dictionary_bundle().unwrap();
    let ocr = demo_ocr_response();
    let (items, posture, _date) = parse_ocr_response(&ocr, &dict);

    assert_eq!(posture, "UPRIGHT");
    assert_eq!(items.len(), 5);
    assert_eq!(items[0].biomarker_code.as_deref(), Some("PAC"));
    assert!((items[0].normalized_value.unwrap() - 22.5).abs() < 1e-6);
    assert_eq!(items[1].biomarker_code.as_deref(), Some("DRC"));
    assert_eq!(items[2].biomarker_code.as_deref(), Some("K_SERUM"));
    assert_eq!(items[3].biomarker_code.as_deref(), Some("PRA"));
}

#[test]
fn headless_ui_loads_demo_saves_and_shows_dashboard() -> Result<(), String> {
    let config = EngineConfig {
        viewport: Size::new(Points(1120.0), Points(800.0)),
        ..EngineConfig::new(Root::of::<TestApp>())
    };
    let mut app = Headless::start(config).map_err(|e| format!("{e:?}"))?;

    app.wait_for("加载模拟化验单")?;
    app.press("加载模拟化验单")?;
    app.wait_for("3. 解析结果核对 (5 项)")?;
    app.wait_for("血浆醛固酮浓度 : 225 pg/mL")?;
    app.wait_for("↑ 高")?;

    app.press("确认保存")?;
    app.wait_for("已事务保存至 SQLite 并更新个人映射记忆")?;

    app.press("趋势看板")?;
    app.wait_for("跨期对比透视表 (已换算至标准单位)")?;
    app.wait_for("方法学隔离提示: PRA (血浆肾素活性, ng/mL/h) 与 DRC (直接肾素浓度, uIU/mL) 已作为两条独立曲线渲染，严禁合并换算。")?;
    Ok(())
}
