const BACKDROP_ENV: &str = "XRTRANSLATE_WINDOW_BACKDROP";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowBackdrop {
    None,
    Transparent,
    Acrylic,
    Mica,
}

impl WindowBackdrop {
    pub(crate) fn from_environment() -> Self {
        std::env::var(BACKDROP_ENV)
            .ok()
            .as_deref()
            .and_then(Self::parse)
            .unwrap_or_default()
    }

    pub(crate) const fn uses_transparent_surface(self) -> bool {
        !matches!(self, Self::None)
    }

    pub(crate) fn clear_color(self) -> [f32; 4] {
        match self {
            Self::None => eframe::egui::Color32::from_rgb(248, 250, 252),
            Self::Transparent => eframe::egui::Color32::TRANSPARENT,
            // The system backdrop supplies the frosted material.  Keep the
            // WGPU surface fully transparent so its entire initial backing
            // store is cleared instead of leaving a white compositor tile.
            Self::Acrylic | Self::Mica => eframe::egui::Color32::TRANSPARENT,
        }
        .to_normalized_gamma_f32()
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "transparent" => Some(Self::Transparent),
            "acrylic" => Some(Self::Acrylic),
            "mica" => Some(Self::Mica),
            _ => None,
        }
    }
}

impl Default for WindowBackdrop {
    fn default() -> Self {
        if cfg!(windows) {
            Self::Acrylic
        } else {
            Self::None
        }
    }
}

pub(crate) fn apply(
    context: &eframe::CreationContext<'_>,
    requested: WindowBackdrop,
) -> Result<(), String> {
    #[cfg(windows)]
    return windows_impl::apply(context, requested);

    #[cfg(not(windows))]
    {
        let _ = (context, requested);
        Ok(())
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::WindowBackdrop;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{sync::Arc, thread, time::Duration};
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute},
        UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WS_EX_LAYERED,
        },
    };
    use winit::platform::windows::{BackdropType, WindowExtWindows};

    const DWMWA_REDIRECTIONBITMAP_ALPHA: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(39);

    pub(super) fn apply(
        context: &eframe::CreationContext<'_>,
        requested: WindowBackdrop,
    ) -> Result<(), String> {
        let window = context
            .winit_window()
            .ok_or_else(|| "eframe did not expose its winit window".to_owned())?;
        let backdrop = match requested {
            WindowBackdrop::None | WindowBackdrop::Transparent => BackdropType::None,
            WindowBackdrop::Acrylic => BackdropType::TransientWindow,
            WindowBackdrop::Mica => BackdropType::MainWindow,
        };
        let handle = window
            .window_handle()
            .map_err(|error| format!("window handle is unavailable: {error}"))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err("eframe did not create a Win32 window".into());
        };

        let hwnd = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
        if requested.uses_transparent_surface() {
            enable_layered_style(hwnd);
        }
        window.set_system_backdrop(backdrop);
        set_redirection_bitmap_alpha(hwnd, requested.uses_transparent_surface())
            .map_err(|error| format!("unable to configure redirection bitmap alpha: {error}"))?;

        // Keep the root hidden while the transparent surface is initialized. Do
        // not invalidate the window before WGPU has submitted its first clear
        // frame: doing so can make DWM preserve the pre-backdrop white surface.
        queue_surface_reconfigure(context, window);
        if requested.uses_transparent_surface() {
            // Windows can retain the first decorated redirection frame for a
            // transparent window. Restore decorations only after eframe presents
            // its first visible frame; doing it during creation recreates that
            // stale frame and leaves a ghost after maximize/resize.
            restore_decorations_after_first_visible(Arc::clone(window), context.egui_ctx.clone());
        }
        window.request_redraw();
        Ok(())
    }

    fn restore_decorations_after_first_visible(
        window: Arc<winit::window::Window>,
        egui_ctx: eframe::egui::Context,
    ) {
        thread::spawn(move || {
            for _ in 0..120 {
                if window.is_visible() == Some(true) {
                    // `is_visible` flips immediately after eframe presents its
                    // first frame. Give DWM one event-loop turn to commit that
                    // transparent surface before changing the window frame.
                    thread::sleep(Duration::from_millis(100));
                    egui_ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Decorations(true));
                    window.request_redraw();
                    return;
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    fn enable_layered_style(hwnd: HWND) {
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let layered = style | WS_EX_LAYERED.0 as isize;
            if layered != style {
                let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, layered);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
    }

    fn queue_surface_reconfigure(
        context: &eframe::CreationContext<'_>,
        window: &winit::window::Window,
    ) {
        // eframe creates/configures the WGPU surface before the app creation
        // callback (where the DWM alpha attribute is available). Queue a pair of
        // size commands so eframe reconfigures that first surface after its root
        // viewport is registered, while the window is still hidden.
        let size = window.inner_size().to_logical::<f64>(window.scale_factor());
        if size.width > 1.0 && size.height > 0.0 {
            let logical_size = eframe::egui::vec2(size.width as f32, size.height as f32);
            context
                .egui_ctx
                .send_viewport_cmd(eframe::egui::ViewportCommand::InnerSize(
                    logical_size + eframe::egui::vec2(1.0, 0.0),
                ));
            context
                .egui_ctx
                .send_viewport_cmd(eframe::egui::ViewportCommand::InnerSize(logical_size));
        }
    }

    fn set_redirection_bitmap_alpha(hwnd: HWND, enabled: bool) -> windows::core::Result<()> {
        let enabled = i32::from(enabled);
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_REDIRECTIONBITMAP_ALPHA,
                std::ptr::from_ref(&enabled).cast(),
                std::mem::size_of_val(&enabled) as u32,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backdrop_names_are_case_insensitive() {
        assert_eq!(
            WindowBackdrop::parse(" Acrylic "),
            Some(WindowBackdrop::Acrylic)
        );
        assert_eq!(WindowBackdrop::parse("MICA"), Some(WindowBackdrop::Mica));
        assert_eq!(WindowBackdrop::parse("unknown"), None);
    }

    #[test]
    fn only_none_uses_an_opaque_render_surface() {
        assert!(!WindowBackdrop::None.uses_transparent_surface());
        assert!(WindowBackdrop::Transparent.uses_transparent_surface());
        assert!(WindowBackdrop::Acrylic.uses_transparent_surface());
        assert!(WindowBackdrop::Mica.uses_transparent_surface());
    }

    #[test]
    fn system_backdrop_clear_color_is_transparent() {
        for backdrop in [WindowBackdrop::Acrylic, WindowBackdrop::Mica] {
            assert_eq!(backdrop.clear_color(), [0.0, 0.0, 0.0, 0.0]);
        }
    }
}
