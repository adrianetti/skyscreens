//! skyscreens headset app. Phase 0: OpenXR + Vulkan, passthrough, one test screen.

#[cfg(target_os = "android")]
mod app;
#[cfg(target_os = "android")]
mod gpu;
mod pattern;
#[cfg(target_os = "android")]
mod platform;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android: android_activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("skyscreens"));
    if let Err(e) = app::run(android) {
        log::error!("fatal: {e}");
    }
}
