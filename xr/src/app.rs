//! OpenXR + Vulkan setup and the frame loop. No projection layer and no 3D rendering:
//! each screen is a compositor layer, so the GPU does almost nothing per frame.

use crate::{gpu::Uploader, pattern::test_pattern, platform::Platform, Result};
use android_activity::{AndroidApp, MainEvent, PollEvent};
use ash::vk::{self, Handle};
use openxr as xr;
use std::time::Duration;

/// Phase 0 test screen: the code screen's default size for 25 pixels per degree.
const SCREEN_W: u32 = 1616;
const SCREEN_H: u32 = 1008;
const SCREEN_DEGREES: f32 = 58.0;
const DISTANCE_M: f32 = 1.5;

pub fn run(android: AndroidApp) -> Result<()> {
    let entry = unsafe { xr::Entry::load()? };
    entry.initialize_android_loader()?;
    let available = entry.enumerate_extensions()?;
    let platform = Platform::detect(&available);
    log::info!("platform: {platform:?}");
    log::info!("unrecognized extensions: {:?}", available.other);

    let mut enabled = xr::ExtensionSet::default();
    enabled.khr_vulkan_enable2 = true;
    enabled.khr_android_create_instance = true;
    platform.enable(&mut enabled);
    let instance = entry.create_instance(
        &xr::ApplicationInfo { application_name: "skyscreens", application_version: 1, engine_name: "skyscreens", engine_version: 1, api_version: xr::Version::new(1, 0, 0) },
        &enabled,
        &[],
    )?;
    let props = instance.properties()?;
    log::info!("runtime: {} {}", props.runtime_name, props.runtime_version);
    let system = instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;
    let _requirements = instance.graphics_requirements::<xr::Vulkan>(system)?;

    // Vulkan instance and device, created through OpenXR so the runtime can add what it needs.
    let vk_entry = unsafe { ash::Entry::load()? };
    let app_info = vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 1, 0));
    let get_proc = unsafe { std::mem::transmute(vk_entry.static_fn().get_instance_proc_addr) };
    let vk_instance = unsafe {
        let info = vk::InstanceCreateInfo::default().application_info(&app_info);
        let raw = instance.create_vulkan_instance(system, get_proc, &info as *const _ as *const _)?.map_err(vk::Result::from_raw)?;
        ash::Instance::load(vk_entry.static_fn(), vk::Instance::from_raw(raw as _))
    };
    let pdev = vk::PhysicalDevice::from_raw(unsafe { instance.vulkan_graphics_device(system, vk_instance.handle().as_raw() as _)? } as _);
    let queue_family = unsafe { vk_instance.get_physical_device_queue_family_properties(pdev) }
        .iter()
        .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        .ok_or("no graphics queue")? as u32;
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family).queue_priorities(&priorities)];
    let device = unsafe {
        let info = vk::DeviceCreateInfo::default().queue_create_infos(&queues);
        let raw = instance.create_vulkan_device(system, get_proc, pdev.as_raw() as _, &info as *const _ as *const _)?.map_err(vk::Result::from_raw)?;
        ash::Device::load(vk_instance.fp_v1_0(), vk::Device::from_raw(raw as _))
    };

    let (session, mut frame_wait, mut frame_stream) = unsafe {
        instance.create_session::<xr::Vulkan>(
            system,
            &xr::vulkan::SessionCreateInfo {
                instance: vk_instance.handle().as_raw() as _,
                physical_device: pdev.as_raw() as _,
                device: device.handle().as_raw() as _,
                queue_family_index: queue_family,
                queue_index: 0,
            },
        )?
    };
    let space = session.create_reference_space(xr::ReferenceSpaceType::LOCAL, xr::Posef::IDENTITY)?;

    let passthrough = if platform.passthrough {
        let p = session.create_passthrough(xr::PassthroughFlagsFB::IS_RUNNING_AT_CREATION)?;
        let layer = session.create_passthrough_layer(&p, xr::PassthroughFlagsFB::IS_RUNNING_AT_CREATION, xr::PassthroughLayerPurposeFB::RECONSTRUCTION)?;
        Some((p, layer))
    } else {
        None
    };

    // The Mac sends BGRA; use a BGRA swapchain when offered so pixels upload unchanged.
    let formats = session.enumerate_swapchain_formats()?;
    let bgra = formats.contains(&(vk::Format::B8G8R8A8_SRGB.as_raw() as u32));
    let format = if bgra { vk::Format::B8G8R8A8_SRGB } else { vk::Format::R8G8B8A8_SRGB };
    log::info!("swapchain format {format:?}");
    let mut swapchain = session.create_swapchain(&xr::SwapchainCreateInfo {
        create_flags: xr::SwapchainCreateFlags::EMPTY,
        usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::TRANSFER_DST | xr::SwapchainUsageFlags::SAMPLED,
        format: format.as_raw() as u32,
        sample_count: 1,
        width: SCREEN_W,
        height: SCREEN_H,
        face_count: 1,
        array_size: 1,
        mip_count: 1,
    })?;
    let images: Vec<vk::Image> = swapchain.enumerate_images()?.into_iter().map(vk::Image::from_raw).collect();

    let mut pixels = test_pattern(SCREEN_W as usize, SCREEN_H as usize);
    if !bgra {
        pixels.chunks_exact_mut(4).for_each(|p| p.swap(0, 2));
    }
    let uploader = Uploader::new(&vk_instance, pdev, device.clone(), queue_family, pixels.len())?;
    let full = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent: vk::Extent2D { width: SCREEN_W, height: SCREEN_H } };
    for _ in &images {
        let i = swapchain.acquire_image()? as usize;
        swapchain.wait_image(xr::Duration::INFINITE)?;
        uploader.upload(images[i], vk::ImageLayout::UNDEFINED, full, &pixels)?;
        swapchain.release_image()?;
    }

    let width_m = 2.0 * DISTANCE_M * (SCREEN_DEGREES.to_radians() / 2.0).tan();
    let height_m = width_m * SCREEN_H as f32 / SCREEN_W as f32;
    let mut events = xr::EventDataBuffer::new();
    let (mut running, mut quit) = (false, false);
    while !quit {
        android.poll_events(Some(Duration::ZERO), |e| {
            if let PollEvent::Main(MainEvent::Destroy) = e {
                quit = true;
            }
        });
        while let Some(event) = instance.poll_event(&mut events)? {
            match event {
                xr::Event::SessionStateChanged(s) => match s.state() {
                    xr::SessionState::READY => {
                        session.begin(xr::ViewConfigurationType::PRIMARY_STEREO)?;
                        running = true;
                        if platform.refresh_rate {
                            log::info!("refresh rates: {:?}", session.enumerate_display_refresh_rates()?);
                            session.request_display_refresh_rate(90.0)?;
                        }
                    }
                    xr::SessionState::STOPPING => {
                        session.end()?;
                        running = false;
                    }
                    xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => quit = true,
                    _ => {}
                },
                xr::Event::InstanceLossPending(_) => quit = true,
                _ => {}
            }
        }
        if !running {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        let state = frame_wait.wait()?;
        frame_stream.begin()?;
        if !state.should_render {
            frame_stream.end(state.predicted_display_time, xr::EnvironmentBlendMode::OPAQUE, &[])?;
            continue;
        }
        let quad = xr::CompositionLayerQuad::new()
            .space(&space)
            .eye_visibility(xr::EyeVisibility::BOTH)
            .sub_image(
                xr::SwapchainSubImage::new()
                    .swapchain(&swapchain)
                    .image_array_index(0)
                    .image_rect(xr::Rect2Di { offset: xr::Offset2Di { x: 0, y: 0 }, extent: xr::Extent2Di { width: SCREEN_W as i32, height: SCREEN_H as i32 } }),
            )
            .pose(xr::Posef { orientation: xr::Quaternionf::IDENTITY, position: xr::Vector3f { x: 0.0, y: 0.0, z: -DISTANCE_M } })
            .size(xr::Extent2Df { width: width_m, height: height_m });
        let underlay = passthrough.as_ref().map(|(_, layer)| xr::sys::CompositionLayerPassthroughFB {
            ty: xr::sys::CompositionLayerPassthroughFB::TYPE,
            next: std::ptr::null(),
            flags: xr::CompositionLayerFlags::BLEND_TEXTURE_SOURCE_ALPHA,
            space: xr::sys::Space::NULL,
            layer_handle: *layer.inner(),
        });
        let mut layers: Vec<&xr::CompositionLayerBase<xr::Vulkan>> = Vec::with_capacity(2);
        if let Some(u) = &underlay {
            // CompositionLayerBase is a transparent wrapper over the layer header.
            layers.push(unsafe { &*(u as *const xr::sys::CompositionLayerPassthroughFB as *const xr::CompositionLayerBase<xr::Vulkan>) });
        }
        layers.push(&quad);
        frame_stream.end(state.predicted_display_time, xr::EnvironmentBlendMode::OPAQUE, &layers)?;
    }
    drop(uploader);
    Ok(())
}
