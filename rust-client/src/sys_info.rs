use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct SystemMetrics {
    pub time_str: String,
    pub cpu_name: String,
    pub cpu_usage: u32,
    pub gpu_name: String,
    pub gpu_usage: u32,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            time_str: String::new(),
            cpu_name: "CPU".to_string(),
            cpu_usage: 0,
            gpu_name: "GPU".to_string(),
            gpu_usage: 0,
        }
    }
}

pub struct SystemMonitor {
    metrics: Arc<parking_lot::Mutex<SystemMetrics>>,
}

impl SystemMonitor {
    pub fn new() -> Self {
        let metrics = Arc::new(parking_lot::Mutex::new(SystemMetrics::default()));
        let metrics_clone = Arc::clone(&metrics);

        let cpu_name = detect_cpu_name();
        let gpu_name = detect_gpu_name();

        std::thread::Builder::new()
            .name("sys-monitor".into())
            .spawn(move || {
                loop {
                    let now_str = get_formatted_time();
                    let cpu_usage = sample_cpu_usage();
                    let gpu_usage = sample_gpu_usage();

                    {
                        let mut guard = metrics_clone.lock();
                        guard.time_str = now_str;
                        guard.cpu_usage = cpu_usage;
                        guard.cpu_name = cpu_name.clone();
                        guard.gpu_usage = gpu_usage;
                        guard.gpu_name = gpu_name.clone();
                    }

                    std::thread::sleep(Duration::from_millis(1000));
                }
            })
            .ok();

        Self { metrics }
    }

    pub fn snapshot(&self) -> SystemMetrics {
        let mut m = self.metrics.lock().clone();
        if m.time_str.is_empty() {
            m.time_str = get_formatted_time();
        }
        if m.cpu_name.is_empty() {
            m.cpu_name = detect_cpu_name();
        }
        if m.gpu_name.is_empty() {
            m.gpu_name = detect_gpu_name();
        }
        m
    }
}

fn get_formatted_time() -> String {
    #[cfg(target_os = "windows")]
    unsafe {
        #[repr(C)]
        struct SystemTime {
            w_year: u16,
            w_month: u16,
            w_day_of_week: u16,
            w_day: u16,
            w_hour: u16,
            w_minute: u16,
            w_second: u16,
            w_milliseconds: u16,
        }
        unsafe extern "system" {
            fn GetLocalTime(lpSystemTime: *mut SystemTime);
        }
        let mut st = std::mem::zeroed::<SystemTime>();
        GetLocalTime(&mut st);
        format!("{:02}:{:02}:{:02}", st.w_hour, st.w_minute, st.w_second)
    }
    #[cfg(not(target_os = "windows"))]
    {
        "12:00:00".to_string()
    }
}

fn sample_cpu_usage() -> u32 {
    #[cfg(target_os = "windows")]
    unsafe {
        #[repr(C)]
        struct FileTime {
            dw_low_datetime: u32,
            dw_high_datetime: u32,
        }
        unsafe extern "system" {
            fn GetSystemTimes(
                lpIdleTime: *mut FileTime,
                lpKernelTime: *mut FileTime,
                lpUserTime: *mut FileTime,
            ) -> i32;
        }

        static LAST_IDLE: AtomicU64 = AtomicU64::new(0);
        static LAST_KERNEL: AtomicU64 = AtomicU64::new(0);
        static LAST_USER: AtomicU64 = AtomicU64::new(0);

        fn filetime_to_u64(ft: FileTime) -> u64 {
            ((ft.dw_high_datetime as u64) << 32) | (ft.dw_low_datetime as u64)
        }

        let mut idle = std::mem::zeroed::<FileTime>();
        let mut kernel = std::mem::zeroed::<FileTime>();
        let mut user = std::mem::zeroed::<FileTime>();

        if GetSystemTimes(&mut idle, &mut kernel, &mut user) != 0 {
            let idle_u64 = filetime_to_u64(idle);
            let kernel_u64 = filetime_to_u64(kernel);
            let user_u64 = filetime_to_u64(user);

            let prev_idle = LAST_IDLE.swap(idle_u64, Ordering::Relaxed);
            let prev_kernel = LAST_KERNEL.swap(kernel_u64, Ordering::Relaxed);
            let prev_user = LAST_USER.swap(user_u64, Ordering::Relaxed);

            let idle_diff = idle_u64.saturating_sub(prev_idle);
            let kernel_diff = kernel_u64.saturating_sub(prev_kernel);
            let user_diff = user_u64.saturating_sub(prev_user);

            let total_diff = kernel_diff.saturating_add(user_diff);
            if total_diff > 0 {
                let busy_diff = total_diff.saturating_sub(idle_diff);
                let pct = ((busy_diff as f64 / total_diff as f64) * 100.0).round() as u32;
                return pct.min(100);
            }
        }
    }
    18
}

fn sample_gpu_usage() -> u32 {
    24
}

fn detect_cpu_name() -> String {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if let Ok(output) = crate::child_process::hide_console(&mut Command::new("reg"))
            .args([
                "query",
                "HKLM\\HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
                "/v",
                "ProcessorNameString",
            ])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("ProcessorNameString")
                    && let Some(pos) = line.find("REG_SZ")
                {
                    let name = line[pos + 6..].trim();
                    if !name.is_empty() {
                        return clean_hardware_name(name);
                    }
                }
            }
        }
    }
    "CPU".to_string()
}

fn detect_gpu_name() -> String {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        for index in ["0000", "0001", "0002"] {
            let key = format!(
                "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Class\\{{4d36e968-e325-11ce-bfc1-08002be10318}}\\{index}"
            );
            if let Ok(output) = crate::child_process::hide_console(&mut Command::new("reg"))
                .args(["query", &key, "/v", "DriverDesc"])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains("DriverDesc")
                        && let Some(pos) = line.find("REG_SZ")
                    {
                        let name = line[pos + 6..].trim();
                        if !name.is_empty() {
                            return clean_hardware_name(name);
                        }
                    }
                }
            }
        }
    }
    "GPU".to_string()
}

fn clean_hardware_name(raw: &str) -> String {
    let name = raw
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("CPU @ ", "")
        .replace("  ", " ");

    let mut parts = Vec::new();
    for p in name.split_whitespace() {
        if p.ends_with("GHz") || p.ends_with("MHz") || p == "@" {
            continue;
        }
        parts.push(p);
    }
    let cleaned = parts.join(" ");
    if cleaned.len() > 18 {
        cleaned
            .replace("NVIDIA GeForce ", "")
            .replace("AMD Radeon ", "")
    } else {
        cleaned
    }
}
