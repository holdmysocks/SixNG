//! System and process memory figures, used to size the 3D map's caches.
//!
//! Budgets scale with the installed RAM so the streamer behaves on a 16 GB
//! laptop as well as a 64 GB workstation. On targets other than Windows and
//! macOS the figures fall back to a conservative 16 GB machine.

#[derive(Clone, Copy, Debug)]
pub struct MemInfo {
    /// Installed physical RAM (bytes).
    pub total: u64,
    /// Physical RAM currently available to programs (bytes).
    pub avail: u64,
    /// This process's committed private memory (bytes).
    pub process: u64,
}

#[cfg(windows)]
pub fn query() -> MemInfo {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
        ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        let ok = GlobalMemoryStatusEx(&mut ms) != 0;
        let mut pc: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        pc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok2 =
            GetProcessMemoryInfo(GetCurrentProcess(), &mut pc as *mut _ as *mut _, pc.cb) != 0;
        MemInfo {
            total: if ok { ms.ullTotalPhys } else { 16 << 30 },
            avail: if ok { ms.ullAvailPhys } else { 8 << 30 },
            process: if ok2 { pc.PrivateUsage as u64 } else { 0 },
        }
    }
}

#[cfg(target_os = "macos")]
pub fn query() -> MemInfo {
    fn sysctl<T: Copy + Default>(name: &std::ffi::CStr) -> Option<T> {
        let mut v = T::default();
        let mut len = std::mem::size_of::<T>();
        let r = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                &mut v as *mut T as *mut _,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        (r == 0 && len == std::mem::size_of::<T>()).then_some(v)
    }
    let total = sysctl::<u64>(c"hw.memsize").unwrap_or(16 << 30);
    // percentage of RAM the kernel considers available (free + reclaimable)
    let avail = sysctl::<u32>(c"kern.memorystatus_level")
        .map_or(8 << 30, |pct| total / 100 * pct.min(100) as u64);
    // physical footprint: what Activity Monitor shows as "Memory"
    let process = unsafe {
        let mut ri: libc::rusage_info_v2 = std::mem::zeroed();
        let ok = libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            &mut ri as *mut _ as *mut _,
        ) == 0;
        if ok {
            ri.ri_phys_footprint
        } else {
            0
        }
    };
    MemInfo {
        total,
        avail,
        process,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn query() -> MemInfo {
    MemInfo {
        total: 16 << 30,
        avail: 8 << 30,
        process: 0,
    }
}

/// Memory limits for the 3D map, derived from the installed RAM.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Above this the streamer stops loading, shrinks new textures and evicts
    /// everything that is not on screen.
    pub soft: u64,
    /// Above this it also drops visible models, farthest first.
    pub hard: u64,
    /// Estimated texture bytes before new loads fall back to 64 px.
    pub textures: u64,
    /// Maximum number of resident models.
    pub models: usize,
    /// Keep at least this much RAM free for the rest of the system.
    pub keep_free: u64,
}

impl Budget {
    pub fn for_total(total: u64) -> Budget {
        const GB: u64 = 1 << 30;
        let total = total.max(4 * GB);
        let gb = total / GB;
        Budget {
            soft: (total as f64 * 0.20) as u64,
            hard: (total as f64 * 0.30) as u64,
            textures: (total / 16).clamp(GB / 2, 3 * GB),
            models: (gb as usize * 120).clamp(800, 8000),
            keep_free: (total / 8).clamp(GB, 4 * GB),
        }
    }

    /// 0 = fine, 1 = over the soft cap (or the system is low on RAM), 2 = over the hard cap.
    pub fn pressure(&self, m: &MemInfo) -> u8 {
        if m.process > self.hard || m.avail < self.keep_free / 2 {
            2
        } else if m.process > self.soft || m.avail < self.keep_free {
            1
        } else {
            0
        }
    }
}

/// This process's GPU memory as the OS reports it (bytes): dedicated video
/// memory and shared (system) memory. Shared use that grows means the card
/// is full and allocations spill into RAM, which is slow to draw from.
/// Sampled about once a second on a background thread (the counters are
/// slow to read); None until the first sample, or where unsupported.
pub fn gpu_process() -> Option<(u64, u64)> {
    use std::sync::atomic::Ordering::Relaxed;
    gpu_sampler_start();
    let (d, s) = (GPU_DED.load(Relaxed), GPU_SHARED.load(Relaxed));
    (d != u64::MAX).then_some((d, s))
}

static GPU_DED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
static GPU_SHARED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(windows)]
fn gpu_sampler_start() {
    static START: std::sync::Once = std::sync::Once::new();
    START.call_once(|| {
        std::thread::spawn(|| {
            use std::sync::atomic::Ordering::Relaxed;
            use windows_sys::Win32::System::Performance::*;
            let wide = |s: &str| {
                s.encode_utf16()
                    .chain(std::iter::once(0))
                    .collect::<Vec<u16>>()
            };
            let pid = std::process::id();
            unsafe {
                let mut q: isize = 0;
                if PdhOpenQueryW(std::ptr::null(), 0, &mut q) != 0 {
                    return;
                }
                // one instance per adapter / memory segment group: sum them
                let mut cs = [0isize; 2];
                for (k, what) in ["Dedicated Usage", "Shared Usage"].iter().enumerate() {
                    let path = wide(&format!("\\GPU Process Memory(pid_{pid}_*)\\{what}"));
                    if PdhAddEnglishCounterW(q, path.as_ptr(), 0, &mut cs[k]) != 0 {
                        return;
                    }
                }
                let sum = |c: isize| -> Option<u64> {
                    let (mut size, mut n) = (0u32, 0u32);
                    let r = PdhGetFormattedCounterArrayW(
                        c,
                        PDH_FMT_LARGE,
                        &mut size,
                        &mut n,
                        std::ptr::null_mut(),
                    );
                    if r != PDH_MORE_DATA || size == 0 {
                        return None;
                    }
                    // the buffer also holds the instance names: allocate it as u64s for alignment
                    let mut buf = vec![0u64; (size as usize).div_ceil(8)];
                    let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                    if PdhGetFormattedCounterArrayW(c, PDH_FMT_LARGE, &mut size, &mut n, items) != 0
                    {
                        return None;
                    }
                    Some(
                        (0..n as usize)
                            .map(|i| (*items.add(i)).FmtValue.Anonymous.largeValue.max(0) as u64)
                            .sum(),
                    )
                };
                loop {
                    if PdhCollectQueryData(q) == 0 {
                        if let (Some(d), Some(s)) = (sum(cs[0]), sum(cs[1])) {
                            GPU_SHARED.store(s, Relaxed);
                            GPU_DED.store(d, Relaxed);
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                }
            }
        });
    });
}

#[cfg(not(windows))]
fn gpu_sampler_start() {}

/// Run the calling (worker) thread below normal priority, so loading never
/// starves the UI thread.
pub fn lower_thread_priority() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

pub fn gb(b: u64) -> f32 {
    b as f32 / (1u64 << 30) as f32
}

impl Budget {
    /// A user-chosen cap (GB) instead of the RAM-derived one; textures and
    /// model counts scale with it.
    pub fn with_cap(mut self, cap_gb: f32) -> Budget {
        let cap = (cap_gb.max(1.0) as f64 * (1u64 << 30) as f64) as u64;
        self.soft = cap;
        self.hard = (cap as f64 * 1.3) as u64;
        self.textures = self.textures.min(cap / 3);
        self.models = self.models.min((cap_gb.max(1.0) * 600.0) as usize);
        self
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn macos_figures_are_real() {
        let m = super::query();
        assert!(
            m.total >= 4 << 30 && m.total % (1 << 20) == 0,
            "total {}",
            m.total
        );
        assert!(m.avail > 0 && m.avail <= m.total, "avail {}", m.avail);
        assert!(
            m.process > 1 << 20 && m.process < m.total,
            "process {}",
            m.process
        );
        eprintln!(
            "total {} GB, avail {} GB, process {} MB",
            m.total >> 30,
            m.avail >> 30,
            m.process >> 20
        );
    }
}
