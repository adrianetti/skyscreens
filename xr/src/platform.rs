use openxr as xr;

#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_excessive_bools, reason = "one flag per optional OpenXR extension")]
pub(crate) struct Platform {
    pub passthrough: bool,
    pub cylinder: bool,
    pub layer_settings: bool,
    pub auto_layer_filter: bool,
    pub surface_swapchain: bool,
    pub refresh_rate: bool,
}

impl Platform {
    pub(crate) fn detect(available: &xr::ExtensionSet) -> Self {
        Self {
            passthrough: available.fb_passthrough,
            cylinder: available.khr_composition_layer_cylinder,
            layer_settings: available.fb_composition_layer_settings,
            auto_layer_filter: available.meta_automatic_layer_filter,
            surface_swapchain: available.khr_android_surface_swapchain,
            refresh_rate: available.fb_display_refresh_rate,
        }
    }

    pub(crate) fn enable(self, enabled: &mut xr::ExtensionSet) {
        enabled.fb_passthrough = self.passthrough;
        enabled.khr_composition_layer_cylinder = self.cylinder;
        enabled.fb_composition_layer_settings = self.layer_settings;
        enabled.meta_automatic_layer_filter = self.auto_layer_filter;
        enabled.fb_display_refresh_rate = self.refresh_rate;
        enabled.khr_android_surface_swapchain = self.surface_swapchain;
    }
}
