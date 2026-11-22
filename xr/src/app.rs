#![expect(unsafe_code, reason = "OpenXR session, JNI and Android surface handles are raw pointers")]

use crate::layout::{DISTANCE_M, TEST_SCREEN_H, TEST_SCREEN_W, quad_size_m};
use crate::runtime::{Runtime, Vulkan};
use crate::{Result, android, gpu::Uploader, net, pattern::test_pattern};
use android_activity::AndroidApp;
use ash::vk::{self, Handle};
use link::Screen;
use ndk::native_window::NativeWindow;
use openxr as xr;
use std::sync::{Arc, mpsc};
use std::time::Duration;

const REFRESH_RATE_HZ: f32 = 90.0;
const IDLE_SLEEP: Duration = Duration::from_millis(50);

pub(crate) fn run(app: &AndroidApp) -> Result<()> {
    android::use_activity_as_context(app);
    let runtime = Runtime::create()?;
    let vulkan = runtime.create_vulkan()?;
    if !android::wait_for_window(app) {
        return Ok(());
    }
    let mut session = XrSession::create(&runtime, &vulkan)?;
    let test_screen = TestScreen::create(&session.session, &vulkan)?;
    let video = VideoLayer::start(app)?;
    FrameLoop { app, runtime: &runtime, xr: &mut session, test_screen: &test_screen, video, running: false }.run()
}

struct XrSession {
    passthrough: Option<(xr::Passthrough, xr::PassthroughLayer)>,
    space: xr::Space,
    frame_stream: xr::FrameStream<xr::Vulkan>,
    frame_wait: xr::FrameWaiter,
    session: xr::Session<xr::Vulkan>,
}

impl XrSession {
    fn create(runtime: &Runtime, vulkan: &Vulkan) -> Result<Self> {
        let info = xr::vulkan::SessionCreateInfo {
            instance: vulkan.instance.handle().as_raw() as *const std::ffi::c_void,
            physical_device: vulkan.physical_device.as_raw() as *const std::ffi::c_void,
            device: vulkan.device.handle().as_raw() as *const std::ffi::c_void,
            queue_family_index: vulkan.queue_family,
            queue_index: 0,
        };
        let (session, frame_wait, frame_stream) =
            unsafe { runtime.instance.create_session::<xr::Vulkan>(runtime.system, &info)? };
        let space = session.create_reference_space(xr::ReferenceSpaceType::LOCAL, xr::Posef::IDENTITY)?;
        let passthrough = if runtime.platform.passthrough {
            let running = xr::PassthroughFlagsFB::IS_RUNNING_AT_CREATION;
            let p = session.create_passthrough(running)?;
            let layer = session.create_passthrough_layer(&p, running, xr::PassthroughLayerPurposeFB::RECONSTRUCTION)?;
            Some((p, layer))
        } else {
            None
        };
        Ok(Self { passthrough, space, frame_stream, frame_wait, session })
    }
}

struct TestScreen {
    swapchain: xr::Swapchain<xr::Vulkan>,
    _uploader: Uploader,
}

impl TestScreen {
    fn create(session: &xr::Session<xr::Vulkan>, vulkan: &Vulkan) -> Result<Self> {
        let formats = session.enumerate_swapchain_formats()?;
        let bgra = formats.contains(&format_code(vk::Format::B8G8R8A8_SRGB)?);
        let format = if bgra { vk::Format::B8G8R8A8_SRGB } else { vk::Format::R8G8B8A8_SRGB };
        log::info!("swapchain format {format:?}");
        let (width, height) = (u32::from(TEST_SCREEN_W), u32::from(TEST_SCREEN_H));
        let mut swapchain = session.create_swapchain(&xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT
                | xr::SwapchainUsageFlags::TRANSFER_DST
                | xr::SwapchainUsageFlags::SAMPLED,
            format: format_code(format)?,
            sample_count: 1,
            width,
            height,
            face_count: 1,
            array_size: 1,
            mip_count: 1,
        })?;
        let images: Vec<vk::Image> = swapchain.enumerate_images()?.into_iter().map(vk::Image::from_raw).collect();

        let mut pixels = test_pattern(usize::from(TEST_SCREEN_W), usize::from(TEST_SCREEN_H));
        if !bgra {
            pixels.chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
        }
        let uploader = Uploader::new(vulkan, pixels.len())?;
        let full = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width, height } };
        for _ in &images {
            let i = swapchain.acquire_image()? as usize;
            swapchain.wait_image(xr::Duration::INFINITE)?;
            uploader.upload(images[i], vk::ImageLayout::UNDEFINED, full, &pixels)?;
            swapchain.release_image()?;
        }
        Ok(Self { swapchain, _uploader: uploader })
    }
}

fn format_code(format: vk::Format) -> Result<u32> {
    Ok(u32::try_from(format.as_raw())?)
}

struct VideoLayer {
    vm: jni::JavaVM,
    shared: Arc<net::Shared>,
    screens: mpsc::Receiver<Vec<Screen>>,
    current: Option<(xr::Swapchain<xr::Vulkan>, u16, u16)>,
}

impl VideoLayer {
    fn start(app: &AndroidApp) -> Result<Self> {
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast())? };
        vm.attach_current_thread_permanently()?;
        let shared = Arc::new(net::Shared::default());
        let (screens_tx, screens) = mpsc::channel();
        net::spawn(Arc::clone(&shared), screens_tx)?;
        Ok(Self { vm, shared, screens, current: None })
    }

    fn apply_screen_updates(&mut self, instance: &xr::Instance, session: &xr::Session<xr::Vulkan>) -> Result<()> {
        while let Ok(screens) = self.screens.try_recv() {
            log::info!("screens: {screens:?}");
            let Some(s) = screens.first() else { continue };
            if self.current.as_ref().map(|v| (v.1, v.2)) == Some((s.width, s.height)) {
                continue;
            }
            self.shared.detach_window();
            drop(self.current.take());
            let (swapchain, surface) = surface_swapchain(instance, session, s.width, s.height)?;
            let env = self.vm.get_env()?;
            let window = unsafe { NativeWindow::from_surface(env.get_raw().cast(), surface.cast()) }
                .ok_or("no window for surface")?;
            self.shared.set_window(window, i32::from(s.width), i32::from(s.height));
            self.current = Some((swapchain, s.width, s.height));
        }
        Ok(())
    }
}

struct FrameLoop<'a> {
    app: &'a AndroidApp,
    runtime: &'a Runtime,
    xr: &'a mut XrSession,
    test_screen: &'a TestScreen,
    video: VideoLayer,
    running: bool,
}

impl FrameLoop<'_> {
    fn run(mut self) -> Result<()> {
        let mut events = xr::EventDataBuffer::new();
        loop {
            if android::pump_events(self.app) || self.handle_xr_events(&mut events)? {
                return Ok(());
            }
            if !self.running {
                std::thread::sleep(IDLE_SLEEP);
                continue;
            }
            self.video.apply_screen_updates(&self.runtime.instance, &self.xr.session)?;
            self.render()?;
        }
    }

    fn handle_xr_events(&mut self, events: &mut xr::EventDataBuffer) -> Result<bool> {
        while let Some(event) = self.runtime.instance.poll_event(events)? {
            #[expect(
                clippy::wildcard_enum_match_arm,
                reason = "the frame loop reacts to two of the runtime's many events"
            )]
            match event {
                xr::Event::SessionStateChanged(change) => {
                    let state = change.state();
                    log::info!("session: {state:?}");
                    if self.on_session_state(state)? {
                        return Ok(true);
                    }
                }
                xr::Event::InstanceLossPending(_) => return Ok(true),
                _ => {}
            }
        }
        Ok(false)
    }

    fn on_session_state(&mut self, state: xr::SessionState) -> Result<bool> {
        match state {
            xr::SessionState::READY => {
                self.xr.session.begin(xr::ViewConfigurationType::PRIMARY_STEREO)?;
                self.running = true;
                if self.runtime.platform.refresh_rate {
                    log::info!("refresh rates: {:?}", self.xr.session.enumerate_display_refresh_rates()?);
                    self.xr.session.request_display_refresh_rate(REFRESH_RATE_HZ)?;
                }
            }
            xr::SessionState::STOPPING => {
                self.xr.session.end()?;
                self.running = false;
            }
            xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => return Ok(true),
            _ => {}
        }
        Ok(false)
    }

    fn render(&mut self) -> Result<()> {
        let state = self.xr.frame_wait.wait()?;
        self.xr.frame_stream.begin()?;
        if !state.should_render {
            self.xr.frame_stream.end(state.predicted_display_time, xr::EnvironmentBlendMode::OPAQUE, &[])?;
            return Ok(());
        }
        let (chain, w, h) = match &self.video.current {
            Some((c, w, h)) => (c, *w, *h),
            None => (&self.test_screen.swapchain, TEST_SCREEN_W, TEST_SCREEN_H),
        };
        let (width_m, height_m) = quad_size_m(w, h);
        let quad = xr::CompositionLayerQuad::new()
            .space(&self.xr.space)
            .eye_visibility(xr::EyeVisibility::BOTH)
            .sub_image(xr::SwapchainSubImage::new().swapchain(chain).image_array_index(0).image_rect(xr::Rect2Di {
                offset: xr::Offset2Di { x: 0, y: 0 },
                extent: xr::Extent2Di { width: i32::from(w), height: i32::from(h) },
            }))
            .pose(xr::Posef {
                orientation: xr::Quaternionf::IDENTITY,
                position: xr::Vector3f { x: 0.0, y: 0.0, z: -DISTANCE_M },
            })
            .size(xr::Extent2Df { width: width_m, height: height_m });
        let underlay = self.xr.passthrough.as_ref().map(|(_, layer)| xr::sys::CompositionLayerPassthroughFB {
            ty: xr::sys::CompositionLayerPassthroughFB::TYPE,
            next: std::ptr::null(),
            flags: xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA,
            space: xr::sys::Space::NULL,
            layer_handle: *layer.inner(),
        });
        let mut layers: Vec<&xr::CompositionLayerBase<'_, xr::Vulkan>> = Vec::with_capacity(2);
        if let Some(u) = &underlay {
            layers.push(unsafe { &*std::ptr::from_ref(u).cast::<xr::CompositionLayerBase<'_, xr::Vulkan>>() });
        }
        layers.push(&quad);
        self.xr.frame_stream.end(state.predicted_display_time, xr::EnvironmentBlendMode::OPAQUE, &layers)?;
        Ok(())
    }
}

fn surface_swapchain(
    instance: &xr::Instance,
    session: &xr::Session<xr::Vulkan>,
    width: u16,
    height: u16,
) -> Result<(xr::Swapchain<xr::Vulkan>, *mut std::ffi::c_void)> {
    let create = instance
        .exts()
        .khr_android_surface_swapchain
        .ok_or("XR_KHR_android_surface_swapchain not enabled")?
        .create_swapchain_android_surface;
    let info = xr::sys::SwapchainCreateInfo {
        ty: xr::sys::SwapchainCreateInfo::TYPE,
        next: std::ptr::null(),
        create_flags: xr::SwapchainCreateFlags::EMPTY,
        usage_flags: xr::SwapchainUsageFlags::SAMPLED | xr::SwapchainUsageFlags::COLOR_ATTACHMENT,
        format: 0,
        sample_count: 0,
        width: u32::from(width),
        height: u32::from(height),
        face_count: 0,
        array_size: 0,
        mip_count: 0,
    };
    let mut handle = xr::sys::Swapchain::NULL;
    let mut surface = std::ptr::null_mut();
    let r = unsafe { create(session.as_raw(), &raw const info, &raw mut handle, &raw mut surface) };
    if r.into_raw() < 0 {
        return Err(format!("xrCreateSwapchainAndroidSurfaceKHR: {r:?}").into());
    }
    Ok((unsafe { xr::Swapchain::from_raw(session.clone(), handle) }, surface.cast()))
}
