use super::{MediaBackend, PlaybackStatus};
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct MpvHandle {
    handle: *mut libmpv_sys::mpv_handle,
}

unsafe impl Send for MpvHandle {}
unsafe impl Sync for MpvHandle {}

impl Drop for MpvHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                libmpv_sys::mpv_terminate_destroy(self.handle);
            }
        }
    }
}

impl MpvHandle {
    fn new() -> Result<Self, String> {
        let handle = unsafe { libmpv_sys::mpv_create() };
        if handle.is_null() {
            return Err("Failed to create MPV handle".into());
        }

        let instance = Self { handle };
        instance.set_property_string("hwdec", "auto-safe")?;
        instance.set_property_string("msg-level", "all=error")?;
        instance.set_property_string("input-default-bindings", "no")?;
        instance.set_property_string("input-vo-keyboard", "no")?;
        instance.set_property_string("osd-font-size", "28")?;
        instance.set_property_string("osd-color", "#FEF08A")?;
        instance.set_property_string("osd-border-color", "#000000")?;
        instance.set_property_string("osd-border-size", "2.5")?;
        instance.set_property_string("osd-shadow-offset", "1.5")?;
        instance.set_property_string("osd-align-y", "bottom")?;
        instance.set_property_string("osd-align-x", "center")?;
        instance.set_property_string("osd-margin-y", "32")?;

        let err = unsafe { libmpv_sys::mpv_initialize(handle) };
        if err < 0 {
            return Err(format!("mpv_initialize failed with error code {}", err));
        }

        Ok(instance)
    }

    fn command(&self, args: &[&str]) -> Result<(), String> {
        let cstrings: Result<Vec<CString>, _> = args.iter().map(|s| CString::new(*s)).collect();
        let cstrings = cstrings.map_err(|e| e.to_string())?;
        let mut ptrs: Vec<*const std::os::raw::c_char> = cstrings.iter().map(|cs| cs.as_ptr()).collect();
        ptrs.push(std::ptr::null());

        let err = unsafe { libmpv_sys::mpv_command(self.handle, ptrs.as_mut_ptr()) };
        if err < 0 {
            let err_str = unsafe {
                let s = libmpv_sys::mpv_error_string(err);
                if s.is_null() {
                    format!("MPV error {}", err)
                } else {
                    CStr::from_ptr(s).to_string_lossy().into_owned()
                }
            };
            return Err(err_str);
        }
        Ok(())
    }

    fn set_property_string(&self, name: &str, val: &str) -> Result<(), String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let c_val = CString::new(val).map_err(|e| e.to_string())?;
        let err = unsafe {
            libmpv_sys::mpv_set_property_string(self.handle, c_name.as_ptr(), c_val.as_ptr())
        };
        if err < 0 {
            Err(format!("Failed to set property {}", name))
        } else {
            Ok(())
        }
    }

    fn set_property_bool(&self, name: &str, val: bool) -> Result<(), String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let mut flag: std::os::raw::c_int = if val { 1 } else { 0 };
        let err = unsafe {
            libmpv_sys::mpv_set_property(
                self.handle,
                c_name.as_ptr(),
                libmpv_sys::mpv_format_MPV_FORMAT_FLAG,
                &mut flag as *mut _ as *mut std::ffi::c_void,
            )
        };
        if err < 0 {
            Err(format!("Failed to set property {}", name))
        } else {
            Ok(())
        }
    }

    fn set_property_i64(&self, name: &str, val: i64) -> Result<(), String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let mut v = val;
        let err = unsafe {
            libmpv_sys::mpv_set_property(
                self.handle,
                c_name.as_ptr(),
                libmpv_sys::mpv_format_MPV_FORMAT_INT64,
                &mut v as *mut _ as *mut std::ffi::c_void,
            )
        };
        if err < 0 {
            Err(format!("Failed to set property {}", name))
        } else {
            Ok(())
        }
    }

    fn set_property_f64(&self, name: &str, val: f64) -> Result<(), String> {
        let c_name = CString::new(name).map_err(|e| e.to_string())?;
        let mut v = val;
        let err = unsafe {
            libmpv_sys::mpv_set_property(
                self.handle,
                c_name.as_ptr(),
                libmpv_sys::mpv_format_MPV_FORMAT_DOUBLE,
                &mut v as *mut _ as *mut std::ffi::c_void,
            )
        };
        if err < 0 {
            Err(format!("Failed to set property {}", name))
        } else {
            Ok(())
        }
    }

    fn get_property_f64(&self, name: &str) -> Option<f64> {
        let c_name = CString::new(name).ok()?;
        let mut val: f64 = 0.0;
        let err = unsafe {
            libmpv_sys::mpv_get_property(
                self.handle,
                c_name.as_ptr(),
                libmpv_sys::mpv_format_MPV_FORMAT_DOUBLE,
                &mut val as *mut _ as *mut std::ffi::c_void,
            )
        };
        if err >= 0 { Some(val) } else { None }
    }

    fn get_property_i64(&self, name: &str) -> Option<i64> {
        let c_name = CString::new(name).ok()?;
        let mut val: i64 = 0;
        let err = unsafe {
            libmpv_sys::mpv_get_property(
                self.handle,
                c_name.as_ptr(),
                libmpv_sys::mpv_format_MPV_FORMAT_INT64,
                &mut val as *mut _ as *mut std::ffi::c_void,
            )
        };
        if err >= 0 { Some(val) } else { None }
    }

    fn get_property_bool(&self, name: &str) -> Option<bool> {
        let c_name = CString::new(name).ok()?;
        let mut val: std::os::raw::c_int = 0;
        let err = unsafe {
            libmpv_sys::mpv_get_property(
                self.handle,
                c_name.as_ptr(),
                libmpv_sys::mpv_format_MPV_FORMAT_FLAG,
                &mut val as *mut _ as *mut std::ffi::c_void,
            )
        };
        if err >= 0 { Some(val != 0) } else { None }
    }

    fn get_property_string(&self, name: &str) -> Option<String> {
        let c_name = CString::new(name).ok()?;
        let ptr = unsafe { libmpv_sys::mpv_get_property_string(self.handle, c_name.as_ptr()) };
        if !ptr.is_null() {
            let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
            unsafe { libmpv_sys::mpv_free(ptr as *mut _) };
            Some(s)
        } else {
            None
        }
    }

    fn poll_event(&self) -> Option<u32> {
        let event_ptr = unsafe { libmpv_sys::mpv_wait_event(self.handle, 0.0) };
        if event_ptr.is_null() {
            return None;
        }
        let event_id = unsafe { (*event_ptr).event_id };
        if event_id == libmpv_sys::mpv_event_id_MPV_EVENT_NONE {
            None
        } else {
            Some(event_id)
        }
    }
}

pub struct MpvBackend {
    mpv: MpvHandle,
    status: PlaybackStatus,
    duration_ms: i64,
    cached_diagnostics: Mutex<super::PlayerDiagnostics>,
    last_diag_poll: Mutex<Instant>,
}

#[cfg(windows)]
fn is_mpv_dll_available() -> bool {
    use windows::Win32::System::LibraryLoader::LoadLibraryW;
    use windows::core::w;
    unsafe {
        if let Ok(handle) = LoadLibraryW(w!("mpv-2.dll")) {
            let _ = windows::Win32::Foundation::FreeLibrary(handle);
            true
        } else if let Ok(handle) = LoadLibraryW(w!("libmpv-2.dll")) {
            let _ = windows::Win32::Foundation::FreeLibrary(handle);
            true
        } else {
            false
        }
    }
}

#[cfg(not(windows))]
fn is_mpv_dll_available() -> bool {
    true
}

impl MpvBackend {
    pub fn new() -> Result<Self, String> {
        if !is_mpv_dll_available() {
            return Err("libmpv runtime (mpv-2.dll) not found".into());
        }

        let mpv = MpvHandle::new()?;

        Ok(Self {
            mpv,
            status: PlaybackStatus::Stopped,
            duration_ms: 0,
            cached_diagnostics: Mutex::new(super::PlayerDiagnostics::default()),
            last_diag_poll: Mutex::new(Instant::now() - Duration::from_secs(10)),
        })
    }
}

impl MediaBackend for MpvBackend {
    fn load_local_file(&mut self, path: PathBuf) -> Result<(), String> {
        self.mpv
            .command(&["loadfile", &path.to_string_lossy()])?;
        self.status = PlaybackStatus::Playing;
        Ok(())
    }

    fn load_stream_url(&mut self, url: String) -> Result<(), String> {
        self.mpv
            .command(&["loadfile", &url])?;
        self.status = PlaybackStatus::Playing;
        Ok(())
    }

    fn play(&mut self) {
        let _ = self.mpv.set_property_bool("pause", false);
        self.status = PlaybackStatus::Playing;
    }

    fn pause(&mut self) {
        let _ = self.mpv.set_property_bool("pause", true);
        self.status = PlaybackStatus::Paused;
    }

    fn stop(&mut self) {
        let _ = self.mpv.command(&["stop"]);
        self.status = PlaybackStatus::Stopped;
    }

    fn seek(&mut self, ms: i64) {
        let secs = ms as f64 / 1000.0;
        let _ = self.mpv.command(&["seek", &secs.to_string(), "absolute"]);
    }

    fn set_volume(&mut self, volume: f32) {
        let _ = self.mpv.set_property_f64("volume", (volume * 100.0) as f64);
    }

    fn set_mute(&mut self, mute: bool) {
        let _ = self.mpv.set_property_bool("mute", mute);
    }

    fn get_time_ms(&self) -> i64 {
        self.mpv
            .get_property_f64("time-pos")
            .map(|t| (t * 1000.0) as i64)
            .unwrap_or(0)
    }

    fn get_duration_ms(&self) -> i64 {
        self.duration_ms
    }

    fn get_status(&self) -> PlaybackStatus {
        self.status
    }

    fn get_diagnostics(&self) -> super::PlayerDiagnostics {
        let mut last_poll = self.last_diag_poll.lock().unwrap();
        let now = Instant::now();
        if now.duration_since(*last_poll) >= Duration::from_millis(250) {
            *last_poll = now;
            let hwdec_current = self.mpv
                .get_property_string("hwdec-current")
                .unwrap_or_else(|| "no".to_string());
            let video_codec = self.mpv
                .get_property_string("video-format")
                .unwrap_or_else(|| self.mpv.get_property_string("video-codec").unwrap_or_default());
            let width = self.mpv
                .get_property_i64("dwidth")
                .unwrap_or_else(|| self.mpv.get_property_i64("video-params/w").unwrap_or(0));
            let height = self.mpv
                .get_property_i64("dheight")
                .unwrap_or_else(|| self.mpv.get_property_i64("video-params/h").unwrap_or(0));
            let fps = self.mpv
                .get_property_f64("estimated-vf-fps")
                .unwrap_or_else(|| self.mpv.get_property_f64("container-fps").unwrap_or(0.0));
            let dropped_frames = self.mpv
                .get_property_i64("vo-drop-frame-count")
                .unwrap_or_else(|| self.mpv.get_property_i64("drop-frame-count").unwrap_or(0));

            let mut cached = self.cached_diagnostics.lock().unwrap();
            *cached = super::PlayerDiagnostics {
                hwdec_current,
                video_codec,
                width,
                height,
                fps,
                dropped_frames,
            };
        }

        self.cached_diagnostics.lock().unwrap().clone()
    }

    fn tick(&mut self) {
        while let Some(event_id) = self.mpv.poll_event() {
            if event_id == libmpv_sys::mpv_event_id_MPV_EVENT_END_FILE {
                self.status = PlaybackStatus::Stopped;
            } else if event_id == libmpv_sys::mpv_event_id_MPV_EVENT_PLAYBACK_RESTART {
                if let Some(duration) = self.mpv.get_property_f64("duration") {
                    self.duration_ms = (duration * 1000.0) as i64;
                }
            }
        }

        if let Some(paused) = self.mpv.get_property_bool("pause") {
            if paused && self.status == PlaybackStatus::Playing {
                self.status = PlaybackStatus::Paused;
            } else if !paused && self.status == PlaybackStatus::Paused {
                self.status = PlaybackStatus::Playing;
            }
        }
    }

    fn attach_native_host(&mut self, host_handle: *mut std::ffi::c_void) {
        if !host_handle.is_null() {
            let wid = host_handle as i64;
            let _ = self.mpv.set_property_i64("wid", wid);
        }
    }

    fn set_osd_subtitle(&mut self, text: &str) {
        if text.is_empty() {
            let _ = self.mpv.command(&["show-text", "", "0", "1"]);
        } else {
            let _ = self.mpv.command(&["show-text", text, "1200", "1"]);
        }
    }

    fn show_osd_title(&mut self, title: &str) {
        let _ = self.mpv.command(&["show-text", title, "3000", "1"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpv_backend_initializes_when_dll_present() {
        if is_mpv_dll_available() {
            let backend = MpvBackend::new();
            assert!(backend.is_ok(), "MpvBackend::new() failed: {:?}", backend.err());
            let b = backend.unwrap();
            assert_eq!(b.get_status(), PlaybackStatus::Stopped);
        }
    }
}
