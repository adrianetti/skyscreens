//! Everything vendor-specific. Features come from what the runtime reports, never from the device name.

use openxr as xr;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // cylinder, filters and surface swapchains are logged now and used from Phase 1
pub struct Platform {
    pub passthrough: bool,
    pub cylinder: bool,
    pub layer_settings: bool,
    pub auto_layer_filter: bool,
    pub surface_swapchain: bool,
    pub refresh_rate: bool,
}

impl Platform {
    pub fn detect(available: &xr::ExtensionSet) -> Platform {
        Platform {
            passthrough: available.fb_passthrough,
            cylinder: available.khr_composition_layer_cylinder,
            layer_settings: available.fb_composition_layer_settings,
            auto_layer_filter: available.meta_automatic_layer_filter,
            surface_swapchain: available.khr_android_surface_swapchain,
            refresh_rate: available.fb_display_refresh_rate,
        }
    }

    pub fn enable(&self, e: &mut xr::ExtensionSet) {
        e.fb_passthrough = self.passthrough;
        e.khr_composition_layer_cylinder = self.cylinder;
        e.fb_composition_layer_settings = self.layer_settings;
        e.meta_automatic_layer_filter = self.auto_layer_filter;
        e.fb_display_refresh_rate = self.refresh_rate;
    }
}
