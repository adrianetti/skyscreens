#![expect(unsafe_code, reason = "ndk_context takes the raw JavaVM and Activity pointers")]

use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use std::time::Duration;

pub(crate) fn use_activity_as_context(app: &AndroidApp) {
    unsafe { ndk_context::release_android_context() };
    unsafe { ndk_context::initialize_android_context(app.vm_as_ptr(), app.activity_as_ptr()) };
}

pub(crate) fn wait_for_window(app: &AndroidApp) -> bool {
    loop {
        let (mut window, mut destroyed) = (false, false);
        app.poll_events(Some(Duration::from_millis(100)), |event| {
            if let PollEvent::Main(main_event) = &event {
                log::info!("android before session: {main_event:?}");
            }
            match event {
                PollEvent::Main(MainEvent::InitWindow { .. }) => window = true,
                PollEvent::Main(MainEvent::Destroy) => destroyed = true,
                PollEvent::Wake | PollEvent::Timeout | PollEvent::Main(_) | _ => {}
            }
        });
        if destroyed {
            return false;
        }
        if window {
            return true;
        }
    }
}

pub(crate) fn pump_events(app: &AndroidApp) -> bool {
    let (mut destroyed, mut input) = (false, false);
    app.poll_events(Some(Duration::ZERO), |event| match event {
        PollEvent::Main(MainEvent::Destroy) => destroyed = true,
        PollEvent::Main(MainEvent::InputAvailable) => input = true,
        PollEvent::Main(main_event) => log::info!("android: {main_event:?}"),
        PollEvent::Wake | PollEvent::Timeout | _ => {}
    });
    if input {
        drain_input_to_stay_responsive(app);
    }
    destroyed
}

fn drain_input_to_stay_responsive(app: &AndroidApp) {
    if let Ok(mut events) = app.input_events_iter() {
        while events.next(|_| InputStatus::Unhandled) {}
    }
}
