#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;

#[cfg(target_os = "linux")]
fn main() -> impl std::process::Termination {
    env_logger::init();
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<nsh_app_lab_scanner::App>()),
        platform::AppOptions {
            title: "离线化验单解析引擎".to_owned(),
            app_id: "com.app.labscanner.nsh".to_owned(),
            size: (1120, 800),
            ..Default::default()
        },
    )
}
