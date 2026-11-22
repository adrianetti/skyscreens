#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod app;
#[cfg(target_os = "android")]
mod decoder;
#[cfg(target_os = "android")]
mod gpu;
pub mod layout;
#[cfg(target_os = "android")]
mod net;
pub mod pattern;
#[cfg(target_os = "android")]
mod platform;
#[cfg(target_os = "android")]
mod runtime;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
#[expect(
    unsafe_code,
    clippy::no_mangle_with_rust_abi,
    clippy::needless_pass_by_value,
    reason = "android-activity calls android_main(AndroidApp) through the Rust ABI"
)]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("skyscreens"),
    );
    if let Err(error) = app::run(&app) {
        log::error!("fatal: {error}");
    }
}
