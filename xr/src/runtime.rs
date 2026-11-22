#![expect(unsafe_code, reason = "loading OpenXR and Vulkan and creating their handles is unsafe")]

use crate::{Result, platform::Platform};
use ash::vk::{self, Handle};
use openxr as xr;
use std::ptr;

pub(crate) struct Runtime {
    pub instance: xr::Instance,
    pub system: xr::SystemId,
    pub platform: Platform,
    _entry: xr::Entry,
}

pub(crate) struct Vulkan {
    pub instance: ash::Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: ash::Device,
    pub queue_family: u32,
    _entry: ash::Entry,
}

const APP_INFO: xr::ApplicationInfo<'static> = xr::ApplicationInfo {
    application_name: "skyscreens",
    application_version: 1,
    engine_name: "skyscreens",
    engine_version: 1,
    api_version: xr::Version::new(1, 0, 0),
};

impl Runtime {
    pub(crate) fn create() -> Result<Self> {
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
        let instance = entry.create_instance(&APP_INFO, &enabled, &[])?;
        let props = instance.properties()?;
        log::info!("runtime: {} {}", props.runtime_name, props.runtime_version);
        let system = instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;
        instance.graphics_requirements::<xr::Vulkan>(system)?;
        Ok(Self { instance, system, platform, _entry: entry })
    }

    pub(crate) fn create_vulkan(&self) -> Result<Vulkan> {
        let entry = unsafe { ash::Entry::load()? };
        let get_proc = unsafe {
            std::mem::transmute::<vk::PFN_vkGetInstanceProcAddr, xr::sys::platform::VkGetInstanceProcAddr>(
                entry.static_fn().get_instance_proc_addr,
            )
        };
        let app_info = vk::ApplicationInfo::default().api_version(vk::make_api_version(0, 1, 1, 0));
        let instance_info = vk::InstanceCreateInfo::default().application_info(&app_info);
        let raw_instance = unsafe {
            self.instance.create_vulkan_instance(self.system, get_proc, ptr::from_ref(&instance_info).cast())?
        }
        .map_err(vk::Result::from_raw)?;
        let instance = unsafe { ash::Instance::load(entry.static_fn(), vk::Instance::from_raw(raw_instance as u64)) };

        let raw_pdev = unsafe {
            self.instance.vulkan_graphics_device(self.system, instance.handle().as_raw() as *const std::ffi::c_void)?
        };
        let physical_device = vk::PhysicalDevice::from_raw(raw_pdev as u64);
        let queue_family = unsafe { instance.get_physical_device_queue_family_properties(physical_device) }
            .iter()
            .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .ok_or("no graphics queue")?;
        let queue_family = u32::try_from(queue_family)?;

        let priorities = [1.0];
        let queues =
            [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family).queue_priorities(&priorities)];
        let device_info = vk::DeviceCreateInfo::default().queue_create_infos(&queues);
        let raw_device = unsafe {
            self.instance.create_vulkan_device(
                self.system,
                get_proc,
                physical_device.as_raw() as *const std::ffi::c_void,
                ptr::from_ref(&device_info).cast(),
            )?
        }
        .map_err(vk::Result::from_raw)?;
        let device = unsafe { ash::Device::load(instance.fp_v1_0(), vk::Device::from_raw(raw_device as u64)) };
        Ok(Vulkan { instance, physical_device, device, queue_family, _entry: entry })
    }
}
