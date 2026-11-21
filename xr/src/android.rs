use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use std::time::Duration;

pub fn use_activity_as_context(app: &AndroidApp) {
    unsafe {
        ndk_context::release_android_context();
        ndk_context::initialize_android_context(app.vm_as_ptr(), app.activity_as_ptr());
    }
}

pub fn wait_for_window(app: &AndroidApp) -> bool {
    loop {
        let (mut window, mut destroyed) = (false, false);
        app.poll_events(Some(Duration::from_millis(100)), |e| {
            if let PollEvent::Main(m) = &e {
                log::info!("android before session: {m:?}");
            }
            match e {
                PollEvent::Main(MainEvent::InitWindow { .. }) => window = true,
                PollEvent::Main(MainEvent::Destroy) => destroyed = true,
                _ => {}
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

pub fn pump_events(app: &AndroidApp) -> bool {
    let (mut destroyed, mut input) = (false, false);
    app.poll_events(Some(Duration::ZERO), |e| match e {
        PollEvent::Main(MainEvent::Destroy) => destroyed = true,
        PollEvent::Main(MainEvent::InputAvailable) => input = true,
        PollEvent::Main(m) => log::info!("android: {m:?}"),
        _ => {}
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
