//! aura-sysmon — the whole machine in one window, btop-style.
//!
//! Left: colored gradient graphs (btop's look) for CPU (plus every core),
//! GPU, network, disk I/O and power; memory split by type; temperatures.
//! Right: processes (sort, filter; right-click → open its folder, end,
//! kill, pause/continue, priority) and "files in memory": every file the
//! running programs have mapped or open, largest first — AI model files
//! (.gguf, .safetensors, …) are marked and announced when they load.
//! Electricity: live watts, and what it costs at your price (editable,
//! saved in ~/.config/aura/sysmon.conf).
//!
//! Everything is read from /proc and /sys on a worker thread, once a
//! second (files every 5 s). Follows the aura theme live.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aura_ui::{theme, widgets};
use egui::{pos2, vec2, Align2, Color32, FontId, Frame, Key, Margin, Rect, RichText, ScrollArea, Sense, Stroke, Ui};

const HISTORY: usize = 150; // samples, one a second
const ALERT: f32 = 0.80;
/// graph heights: cpu, memory, graphics+temps, network+disk, electricity
const DEFAULT_H: [f32; 5] = [110.0, 90.0, 96.0, 110.0, 128.0];
const MIN_H: [f32; 5] = [40.0, 0.0, 64.0, 50.0, 96.0];

// ── reading the machine ─────────────────────────────────────────────────────

fn read(p: impl AsRef<Path>) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn read_num(p: impl AsRef<Path>) -> Option<f64> {
    read(p).trim().parse().ok()
}

/// (busy, total) jiffies for every "cpu*" line: [0] is all cores.
fn cpu_jiffies(stat: &str) -> Vec<(u64, u64)> {
    stat.lines()
        .take_while(|l| l.starts_with("cpu"))
        .map(|l| {
            let v: Vec<u64> = l.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
            let total: u64 = v.iter().take(8).sum();
            let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            (total.saturating_sub(idle), total)
        })
        .collect()
}

/// kB values from /proc/meminfo.
fn meminfo(text: &str) -> HashMap<String, u64> {
    text.lines()
        .filter_map(|l| {
            let (k, rest) = l.split_once(':')?;
            Some((k.to_string(), rest.split_whitespace().next()?.parse().ok()?))
        })
        .collect()
}

/// (used, total) bytes of the filesystem holding `/`.
fn disk_usage() -> (u64, u64) {
    let out = Command::new("df").args(["-B1", "--output=used,size", "/"]).stderr(Stdio::null()).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    let v: Vec<u64> = text.lines().nth(1).unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(1))
}

/// Bytes (read, written) so far by whole disks (nvme0n1, sda — not partitions).
fn disk_io(text: &str) -> (u64, u64) {
    let mut r = (0, 0);
    for l in text.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        let Some(name) = f.get(2) else { continue };
        let whole = (name.starts_with("nvme") && !name.contains('p')) || (name.starts_with("sd") && name.len() == 3);
        if whole {
            r.0 += f.get(5).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0) * 512;
            r.1 += f.get(9).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0) * 512;
        }
    }
    r
}

/// Bytes (received, sent) so far on every interface but loopback.
fn net_io(text: &str) -> (u64, u64) {
    let mut r = (0, 0);
    for l in text.lines().skip(2) {
        let Some((name, rest)) = l.split_once(':') else { continue };
        if name.trim() == "lo" {
            continue;
        }
        let f: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        r.0 += f.first().copied().unwrap_or(0);
        r.1 += f.get(8).copied().unwrap_or(0);
    }
    r
}

#[derive(Clone, Default)]
struct Sensors {
    cpu: Option<f32>,
    gpu: Option<f32>,
    nvme: Option<f32>,
    /// the APU's own power reading (CPU + graphics), watts
    soc_w: Option<f32>,
    gpu_busy: Option<f32>,
    vram: Option<(u64, u64)>,
    gtt: Option<(u64, u64)>,
}

fn sensors() -> Sensors {
    let mut s = Sensors::default();
    let Ok(dirs) = std::fs::read_dir("/sys/class/hwmon") else { return s };
    for d in dirs.flatten() {
        let p = d.path();
        let t = read_num(p.join("temp1_input")).map(|m| m as f32 / 1000.0);
        match read(p.join("name")).trim() {
            "k10temp" | "coretemp" | "zenpower" => s.cpu = t,
            "acpitz" if s.cpu.is_none() => s.cpu = t,
            "nvme" => s.nvme = t,
            "amdgpu" => {
                s.gpu = t;
                s.soc_w = read_num(p.join("power1_input")).or_else(|| read_num(p.join("power1_average"))).map(|u| u as f32 / 1e6);
                let dev = p.join("device");
                s.gpu_busy = read_num(dev.join("gpu_busy_percent")).map(|v| v as f32 / 100.0);
                let pair = |a: &str, b: &str| Some((read_num(dev.join(a))? as u64, read_num(dev.join(b))? as u64));
                s.vram = pair("mem_info_vram_used", "mem_info_vram_total");
                s.gtt = pair("mem_info_gtt_used", "mem_info_gtt_total");
            }
            _ => {}
        }
    }
    s
}

/// Average current frequency of all cores, MHz.
fn cpu_mhz() -> Option<f32> {
    let mut v = Vec::new();
    for e in std::fs::read_dir("/sys/devices/system/cpu").ok()?.flatten() {
        if let Some(k) = read_num(e.path().join("cpufreq/scaling_cur_freq")) {
            v.push(k as f32 / 1000.0);
        }
    }
    (!v.is_empty()).then(|| v.iter().sum::<f32>() / v.len() as f32)
}

/// Whole-laptop watts from the battery, only when it's actually discharging.
fn battery_watts() -> Option<f32> {
    let b = Path::new("/sys/class/power_supply/BAT0");
    if read(b.join("status")).trim() != "Discharging" {
        return None;
    }
    let w = read_num(b.join("power_now")).map(|u| u as f32 / 1e6)
        .or_else(|| Some((read_num(b.join("current_now"))? * read_num(b.join("voltage_now"))?) as f32 / 1e12))?;
    (w > 0.0).then_some(w)
}

#[derive(Clone)]
struct Proc {
    pid: u32,
    name: String,
    rss: u64,
    cpu: f32, // % of one core
    state: char,
    nice: i32,
    threads: u32,
    mine: bool,
}

struct RawProc {
    name: String,
    jiffies: u64,
    rss: u64,
    state: char,
    nice: i32,
    threads: u32,
    uid: u32,
}

fn my_uid() -> u32 {
    read("/proc/self/status").lines().find_map(|l| l.strip_prefix("Uid:")?.split_whitespace().next()?.parse().ok()).unwrap_or(u32::MAX)
}

fn procs_raw() -> HashMap<u32, RawProc> {
    let mut m = HashMap::new();
    let Ok(rd) = std::fs::read_dir("/proc") else { return m };
    for e in rd.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
        let stat = read(format!("/proc/{pid}/stat"));
        // the name sits in parens and may hold spaces; fields resume after ')'
        let (Some(a), Some(b)) = (stat.find('('), stat.rfind(')')) else { continue };
        let f: Vec<&str> = stat.get(b + 2..).unwrap_or("").split_whitespace().collect();
        let n = |i: usize| f.get(i).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
        let uid = std::fs::metadata(format!("/proc/{pid}")).map(|m| std::os::unix::fs::MetadataExt::uid(&m)).unwrap_or(0);
        m.insert(pid, RawProc {
            name: stat[a + 1..b].to_string(),
            jiffies: (n(11) + n(12)) as u64,
            rss: n(21).max(0) as u64 * 4096,
            state: f.first().and_then(|s| s.chars().next()).unwrap_or('?'),
            nice: n(16) as i32,
            threads: n(17) as u32,
            uid,
        });
    }
    m
}

// ── files in memory ─────────────────────────────────────────────────────────

const MODEL_EXT: &[&str] = &["gguf", "ggml", "safetensors", "onnx", "pt", "pth", "ckpt", "llamafile", "mlmodel", "tflite", "bin", "model"];

/// An AI model file: known model extension (".bin"/".model" only when big).
fn is_model(path: &str, size: u64) -> bool {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "bin" | "model" => size > 200 << 20,
        e => MODEL_EXT.contains(&e),
    }
}

#[derive(Clone)]
struct MemFile {
    path: String,
    size: u64,
    /// how much of it sits in RAM right now (page cache), bytes
    cached: u64,
    model: bool,
    kind: Kind,
    who: Vec<String>, // program names
}

/// What the file cache is holding, by kind of file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Kind {
    Model,
    Library,
    Program,
    Media,
    Font,
    Other,
}

impl Kind {
    const ALL: [Kind; 6] = [Kind::Model, Kind::Library, Kind::Program, Kind::Media, Kind::Font, Kind::Other];

    fn of(path: &str, size: u64) -> Kind {
        let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_string()).unwrap_or_default();
        if is_model(path, size) {
            Kind::Model
        } else if name.contains(".so") {
            Kind::Library
        } else if matches!(ext.as_str(), "ttf" | "otf" | "woff" | "woff2" | "pcf") {
            Kind::Font
        } else if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "mp4" | "mkv" | "webm" | "mov" | "mp3" | "flac" | "wav" | "ogg" | "opus") {
            Kind::Media
        } else if ext.is_empty() || matches!(ext.as_str(), "appimage" | "exe" | "bin" | "pak" | "asar" | "dat") || path.contains("/bin/") {
            Kind::Program
        } else {
            Kind::Other
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Model => "AI models",
            Kind::Library => "libraries",
            Kind::Program => "programs' files",
            Kind::Media => "images · video · audio",
            Kind::Font => "fonts",
            Kind::Other => "other open files",
        }
    }

    /// Fixed, clearly different hues (btop-style) — a theme's own accents
    /// are often one family (purplerain: three purples), which made kinds
    /// impossible to tell apart.
    fn color(self) -> Color32 {
        match self {
            Kind::Model => Color32::from_rgb(0xf3, 0x5b, 0x7a),   // red: stands out
            Kind::Library => Color32::from_rgb(0x5c, 0x9d, 0xf5), // blue
            Kind::Program => Color32::from_rgb(0x7d, 0xd8, 0x7d), // green
            Kind::Media => Color32::from_rgb(0xf2, 0xc9, 0x4c),   // yellow
            Kind::Font => Color32::from_rgb(0x4f, 0xd1, 0xc5),    // teal
            Kind::Other => Color32::from_rgb(0xc7, 0x9b, 0xf2),   // lavender
        }
    }
}

/// Bytes of `path` resident in the page cache, asked of the kernel with
/// mincore (works on any readable file, unlike cachestat/fincore).
fn resident(path: &str, size: u64) -> u64 {
    use std::os::unix::io::AsRawFd;
    if size == 0 {
        return 0;
    }
    let Ok(f) = std::fs::File::open(path) else { return 0 };
    let page = 4096usize;
    let len = size as usize;
    unsafe {
        let addr = libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_SHARED, f.as_raw_fd(), 0);
        if addr == libc::MAP_FAILED {
            return 0;
        }
        let mut vec = vec![0u8; len.div_ceil(page)];
        let ok = libc::mincore(addr, len, vec.as_mut_ptr()) == 0;
        libc::munmap(addr, len);
        if !ok {
            return 0;
        }
        vec.iter().filter(|b| **b & 1 == 1).count() as u64 * page as u64
    }
}

/// Files mapped into (or held open by) running programs, ≥ 1 MB, biggest first.
fn files_in_memory(procs: &HashMap<u32, RawProc>) -> Vec<MemFile> {
    let mut by_path: HashMap<String, HashSet<String>> = HashMap::new();
    for (pid, p) in procs {
        for l in read(format!("/proc/{pid}/maps")).lines() {
            if let Some(i) = l.find('/') {
                let path = l[i..].trim_end_matches(" (deleted)");
                by_path.entry(path.to_string()).or_default().insert(p.name.clone());
            }
        }
        if let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) {
            for fd in fds.flatten() {
                if let Ok(t) = std::fs::read_link(fd.path()) {
                    let s = t.to_string_lossy();
                    if s.starts_with('/') && !s.starts_with("/dev") && !s.starts_with("/proc") {
                        by_path.entry(s.to_string()).or_default().insert(p.name.clone());
                    }
                }
            }
        }
    }
    let mut v: Vec<MemFile> = by_path
        .into_iter()
        .filter_map(|(path, who)| {
            let md = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
            let size = md.len();
            (size >= 1 << 20).then(|| {
                let mut who: Vec<String> = who.into_iter().collect();
                who.sort();
                MemFile { model: is_model(&path, size), kind: Kind::of(&path, size), cached: resident(&path, size), path, size, who }
            })
        })
        .collect();
    v.sort_by(|a, b| b.model.cmp(&a.model).then(b.size.cmp(&a.size)));
    v.truncate(300);
    v
}

// ── memory, split by what it holds ──────────────────────────────────────────

/// programs · file cache by kind (6) · cache of closed files · shared · kernel
const SEGS: usize = 10;

/// Bytes per segment. The file cache is split by what the running programs
/// have open or mapped (measured with mincore); the rest of the cache is
/// files that were read earlier and closed.
fn mem_segments(mi: &HashMap<String, u64>, by_kind: &HashMap<Kind, u64>) -> [u64; SEGS] {
    let kb = |k: &str| mi.get(k).copied().unwrap_or(0) * 1024;
    let shmem = kb("Shmem");
    let cache = kb("Cached").saturating_sub(shmem) + kb("Buffers");
    let known: u64 = Kind::ALL.iter().map(|k| by_kind.get(k).copied().unwrap_or(0)).sum();
    // open files can be counted twice (shared pages); never exceed the cache
    let scale = if known > cache { cache as f64 / known as f64 } else { 1.0 };
    let mut out = [0u64; SEGS];
    out[0] = kb("AnonPages");
    let mut sum = 0;
    for (i, k) in Kind::ALL.iter().enumerate() {
        let v = (by_kind.get(k).copied().unwrap_or(0) as f64 * scale) as u64;
        out[1 + i] = v;
        sum += v;
    }
    out[7] = cache.saturating_sub(sum);
    out[8] = shmem;
    out[9] = kb("Slab") + kb("KernelStack") + kb("PageTables");
    out
}

/// (label, color, hint, is it part of the file cache)
fn seg_meta(i: usize) -> (&'static str, Color32, &'static str, bool) {
    match i {
        0 => ("programs", theme::accent(), "memory programs asked for (their own data)", false),
        1..=6 => {
            let k = Kind::ALL[i - 1];
            (k.label(), k.color(), "part of the file cache: files running programs have open", true)
        }
        7 => ("closed files", Color32::from_rgb(0x3f, 0x6e, 0x8c), "files read earlier, kept in RAM in case they're needed again — freed on demand", true),
        8 => ("shared / tmpfs", Color32::from_rgb(0xf5, 0x9e, 0x6b), "shared memory and RAM disks (/tmp, GPU buffers)", false),
        _ => ("kernel", Color32::from_rgb(0x8a, 0x8f, 0xa3), "the kernel's own tables and caches", false),
    }
}

// ── settings (electricity) ──────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct Settings {
    price: f64,       // per kWh
    currency: String,
    extra_w: f64,     // screen, disk, wifi… added to the chip's own reading
    /// width of the process pane — drag the divider, it sticks
    split: f32,
    /// graph heights, drag the handle under each section: cpu, memory,
    /// graphics+temps, network+disk, electricity
    heights: [f32; 5],
}

fn settings_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("aura/sysmon.conf")
}

fn load_settings() -> Settings {
    let mut s = Settings { price: 0.15, currency: "€".into(), extra_w: 8.0, split: 560.0, heights: DEFAULT_H };
    for l in read(settings_path()).lines() {
        match l.split_once('=') {
            Some(("price", v)) => s.price = v.trim().parse().unwrap_or(s.price),
            Some(("currency", v)) => s.currency = v.trim().to_string(),
            Some(("extra_watts", v)) => s.extra_w = v.trim().parse().unwrap_or(s.extra_w),
            Some(("process_pane_width", v)) => s.split = v.trim().parse().unwrap_or(s.split),
            Some(("heights", v)) => {
                for (h, x) in s.heights.iter_mut().zip(v.split(',')) {
                    *h = x.trim().parse().unwrap_or(*h);
                }
            }
            _ => {}
        }
    }
    s
}

fn save_settings(s: &Settings) {
    let p = settings_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let _ = std::fs::write(p, format!(
        "# aura-sysmon — electricity price per kWh, and watts the chip's own\n# reading can't see (screen, disk, wifi); used only when plugged in.\nprice={}\ncurrency={}\nextra_watts={}\nprocess_pane_width={}\nheights={}\n",
        s.price, s.currency, s.extra_w, s.split.round(),
        s.heights.iter().map(|h| h.round().to_string()).collect::<Vec<_>>().join(",")
    ));
}

// ── the sampler ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct Snapshot {
    cpu: VecDeque<f32>,
    cores: Vec<VecDeque<f32>>,
    mem: VecDeque<f32>,
    swap: VecDeque<f32>,
    gpu: VecDeque<f32>,
    net_rx: VecDeque<f32>, // bytes/s
    net_tx: VecDeque<f32>,
    disk_r: VecDeque<f32>,
    disk_w: VecDeque<f32>,
    watts: VecDeque<f32>,
    temp_cpu: VecDeque<f32>,
    temp_gpu: VecDeque<f32>,
    temp_nvme: VecDeque<f32>,
    mhz: Option<f32>,
    sensors: Sensors,
    mem_info: HashMap<String, u64>,
    /// latest memory breakdown (bytes per segment) for the bar
    mem_segs: [u64; SEGS],
    disk_used: u64,
    disk_total: u64,
    on_battery: bool,
    /// watt-hours since the window opened
    wh: f64,
    started: Option<Instant>,
    procs: Vec<Proc>,
    files: Vec<MemFile>,
    new_models: Vec<String>,
    ready: bool,
}

fn push(q: &mut VecDeque<f32>, v: f32) {
    if q.len() == HISTORY {
        q.pop_front();
    }
    q.push_back(v);
}

fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send").args(["-a", "Aura system", title, body]).stdin(Stdio::null()).status();
}

fn sampler(shared: Arc<Mutex<Snapshot>>, settings: Arc<Mutex<Settings>>, ctx: egui::Context) {
    let hz = 100.0; // USER_HZ
    let me = my_uid();
    let mut last_cpu = cpu_jiffies(&read("/proc/stat"));
    let mut last_procs = procs_raw();
    let mut last_disk = disk_io(&read("/proc/diskstats"));
    let mut last_net = net_io(&read("/proc/net/dev"));
    let mut last_t = Instant::now();
    // already over the line when the window opens? you can see it — warn only
    // when it crosses while the monitor is running
    let (mut swap_warned, mut disk_warned) = (true, true);
    let mut seen_models: HashSet<String> = HashSet::new();
    let mut first_files = true;
    let mut by_kind: HashMap<Kind, u64> = HashMap::new();
    let mut tick = 0u64;
    let (mut du, mut dt) = disk_usage();
    shared.lock().unwrap().started = Some(Instant::now());
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let dt_s = last_t.elapsed().as_secs_f32().max(0.1);
        last_t = Instant::now();

        let cpu_now = cpu_jiffies(&read("/proc/stat"));
        let fracs: Vec<f32> = cpu_now
            .iter()
            .zip(&last_cpu)
            .map(|(n, o)| n.0.saturating_sub(o.0) as f32 / n.1.saturating_sub(o.1).max(1) as f32)
            .collect();
        last_cpu = cpu_now;

        let mi = meminfo(&read("/proc/meminfo"));
        let kb = |k: &str| mi.get(k).copied().unwrap_or(0) * 1024;
        let (mt, ma, st, sf) = (kb("MemTotal"), kb("MemAvailable"), kb("SwapTotal"), kb("SwapFree"));
        if tick % 10 == 0 {
            (du, dt) = disk_usage();
        }

        let d = disk_io(&read("/proc/diskstats"));
        let n = net_io(&read("/proc/net/dev"));
        let rate = |a: u64, b: u64| a.saturating_sub(b) as f32 / dt_s;
        let (dr, dw, nr, nt) = (rate(d.0, last_disk.0), rate(d.1, last_disk.1), rate(n.0, last_net.0), rate(n.1, last_net.1));
        (last_disk, last_net) = (d, n);

        let sens = sensors();
        let bat = battery_watts();
        let extra = settings.lock().unwrap().extra_w as f32;
        let watts = bat.or(sens.soc_w.map(|w| w + extra)).unwrap_or(0.0);

        let raw = procs_raw();
        let mut procs: Vec<Proc> = raw
            .iter()
            .filter(|(_, p)| p.rss > 0)
            .map(|(pid, p)| {
                let prev = last_procs.get(pid).map(|o| o.jiffies).unwrap_or(p.jiffies);
                Proc {
                    pid: *pid,
                    name: p.name.clone(),
                    rss: p.rss,
                    cpu: p.jiffies.saturating_sub(prev) as f32 / hz / dt_s * 100.0,
                    state: p.state,
                    nice: p.nice,
                    threads: p.threads,
                    mine: p.uid == me,
                }
            })
            .collect();
        procs.sort_by(|a, b| b.rss.cmp(&a.rss));

        let mut files = None;
        let mut new_models = Vec::new();
        if tick % 5 == 0 {
            let f = files_in_memory(&raw);
            by_kind.clear();
            for m in &f {
                *by_kind.entry(m.kind).or_default() += m.cached;
            }
            for m in f.iter().filter(|m| m.model) {
                if seen_models.insert(m.path.clone()) && !first_files {
                    let name = Path::new(&m.path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    notify("AI model loaded", &format!("{name} ({}) — by {}", size(m.size), m.who.join(", ")));
                    new_models.push(m.path.clone());
                }
            }
            first_files = false;
            files = Some(f);
        }
        last_procs = raw;
        tick += 1;

        let swap_frac = if st > 0 { (st - sf) as f32 / st as f32 } else { 0.0 };
        let disk_frac = du as f32 / dt.max(1) as f32;
        if swap_frac > ALERT && !swap_warned {
            notify("Swap almost full", &format!("{:.0}% of swap in use — close something heavy", swap_frac * 100.0));
        }
        if disk_frac > ALERT && !disk_warned {
            notify("Disk almost full", &format!("{:.0}% of / is used", disk_frac * 100.0));
        }
        // re-arm only once it drops well below, so it doesn't nag
        swap_warned = if swap_warned { swap_frac > ALERT - 0.1 } else { swap_frac > ALERT };
        disk_warned = if disk_warned { disk_frac > ALERT - 0.05 } else { disk_frac > ALERT };

        {
            let mut s = shared.lock().unwrap();
            push(&mut s.cpu, fracs.first().copied().unwrap_or(0.0));
            s.cores.resize(fracs.len().saturating_sub(1), VecDeque::new());
            for (q, v) in s.cores.iter_mut().zip(fracs.iter().skip(1)) {
                push(q, *v);
            }
            push(&mut s.mem, (mt - ma) as f32 / mt.max(1) as f32);
            push(&mut s.swap, swap_frac);
            push(&mut s.gpu, sens.gpu_busy.unwrap_or(0.0));
            push(&mut s.net_rx, nr);
            push(&mut s.net_tx, nt);
            push(&mut s.disk_r, dr);
            push(&mut s.disk_w, dw);
            push(&mut s.watts, watts);
            if let Some(t) = sens.cpu {
                push(&mut s.temp_cpu, t);
            }
            push(&mut s.temp_gpu, sens.gpu.unwrap_or(0.0));
            push(&mut s.temp_nvme, sens.nvme.unwrap_or(0.0));
            s.wh += watts as f64 * dt_s as f64 / 3600.0;
            s.on_battery = bat.is_some();
            s.mhz = cpu_mhz();
            s.sensors = sens;
            s.mem_segs = mem_segments(&mi, &by_kind);
            s.mem_info = mi;
            (s.disk_used, s.disk_total) = (du, dt);
            s.procs = procs;
            if let Some(f) = files {
                s.files = f;
            }
            s.new_models.extend(new_models);
            s.ready = true;
        }
        ctx.request_repaint();
    }
}

// ── formatting ──────────────────────────────────────────────────────────────

fn size(b: u64) -> String {
    let b = b as f64;
    if b >= (1u64 << 30) as f64 {
        format!("{:.1} GB", b / (1u64 << 30) as f64)
    } else if b >= (1u64 << 20) as f64 {
        format!("{:.0} MB", b / (1u64 << 20) as f64)
    } else {
        format!("{:.0} kB", b / 1024.0)
    }
}

fn speed(bps: f32) -> String {
    format!("{}/s", size(bps as u64))
}

fn pct(f: f32) -> String {
    format!("{:.0}%", f * 100.0)
}

// ── drawing: btop-style graphs ──────────────────────────────────────────────

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Color at height `t` (0 bottom → 1 top) along a 3-stop gradient.
fn grad(stops: [Color32; 3], t: f32) -> Color32 {
    if t < 0.5 { lerp(stops[0], stops[1], t * 2.0) } else { lerp(stops[1], stops[2], (t - 0.5) * 2.0) }
}

/// Green → yellow → red, btop's CPU gradient, from the theme's own colors.
fn heat() -> [Color32; 3] {
    [theme::ok(), theme::warn(), theme::bad()]
}

/// btop's graph: one column per sample, each column built from small cells
/// colored by height, so the tall peaks turn hot. `data` is 0..=1.
fn btop_graph(ui: &Ui, rect: Rect, data: &VecDeque<f32>, stops: [Color32; 3], flip: bool) {
    let p = ui.painter();
    let n = HISTORY;
    let col_w = rect.width() / n as f32;
    let cell = 3.0_f32;
    let rows = (rect.height() / cell).floor().max(1.0) as usize;
    let start = n - data.len().min(n);
    for (i, v) in data.iter().enumerate() {
        let x = rect.left() + (start + i) as f32 * col_w;
        let h = (v.clamp(0.0, 1.0) * rows as f32).ceil() as usize;
        for r in 0..h {
            let t = (r as f32 + 0.5) / rows as f32;
            let y = if flip { rect.top() + r as f32 * cell } else { rect.bottom() - (r + 1) as f32 * cell };
            let cr = Rect::from_min_size(pos2(x, y + 0.5), vec2((col_w - 0.6).max(0.8), cell - 1.0));
            p.rect_filled(cr, 0.0, grad(stops, t));
        }
    }
}

/// A box with a title in its top border, like btop's panels. Returns inner rect.
fn panel(ui: &mut Ui, title: &str, right: &str, h: f32) -> Rect {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let p = ui.painter();
    p.rect_stroke(rect.shrink(0.5), theme::radius(), Stroke::new(1.0_f32, theme::surface1()), egui::StrokeKind::Inside);
    let tg = p.layout_no_wrap(format!(" {title} "), FontId::monospace(12.0), theme::accent());
    let tr = Rect::from_min_size(rect.left_top() + vec2(10.0, -7.0), tg.size());
    p.rect_filled(tr, 0.0, theme::base());
    p.galley(tr.min, tg, theme::accent());
    if !right.is_empty() {
        let rg = p.layout_no_wrap(format!(" {right} "), FontId::monospace(12.0), theme::text());
        let rr = Rect::from_min_size(pos2(rect.right() - 10.0 - rg.size().x, rect.top() - 7.0), rg.size());
        p.rect_filled(rr, 0.0, theme::base());
        p.galley(rr.min, rg, theme::text());
    }
    ui.add_space(12.0);
    rect.shrink2(vec2(10.0, 12.0))
}

/// A drag handle under a section (herdr-style split): drag it up or down to
/// resize the section above. Returns true when a drag just ended (save).
fn splitter(ui: &mut Ui, id: &str, h: &mut f32, min: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::drag());
    let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeVertical);
    let _ = id;
    if resp.dragged() {
        *h = (*h + resp.drag_delta().y).clamp(min, 900.0);
    }
    let hot = resp.hovered() || resp.dragged();
    let p = ui.painter();
    if hot {
        p.hline(rect.x_range(), rect.center().y, Stroke::new(2.0_f32, theme::accent()));
    }
    for dx in [-8.0_f32, 0.0, 8.0] {
        p.circle_filled(pos2(rect.center().x + dx, rect.center().y), 1.6, if hot { theme::accent() } else { theme::surface2() });
    }
    resp.drag_stopped()
}

fn text(ui: &Ui, at: egui::Pos2, align: Align2, s: &str, c: Color32) {
    ui.painter().text(at, align, s, FontId::monospace(12.0), c);
}

/// A small horizontal meter with a heat gradient, and a % after it.
fn meter(ui: &Ui, rect: Rect, f: f32) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::surface0());
    let cells = (rect.width() / 4.0) as usize;
    let lit = (f.clamp(0.0, 1.0) * cells as f32).round() as usize;
    for i in 0..lit {
        let r = Rect::from_min_size(pos2(rect.left() + i as f32 * 4.0, rect.top()), vec2(3.0, rect.height()));
        p.rect_filled(r, 0.0, grad(heat(), i as f32 / cells as f32));
    }
}

fn max_of(q: &VecDeque<f32>) -> f32 {
    q.iter().copied().fold(0.0, f32::max)
}

// ── the window ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Mem,
    Cpu,
    Name,
    Pid,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Procs,
    Files,
}

struct App {
    data: Arc<Mutex<Snapshot>>,
    settings: Arc<Mutex<Settings>>,
    edit: Settings,
    sort: Sort,
    tab: Tab,
    filter: String,
    status: String,
    dragging_split: bool,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
    mem_open: bool, // memory breakdown tree open? closed = the graph gets the space
}

fn open_folder(path: &Path) {
    let dir = if path.is_dir() { path.to_path_buf() } else { path.parent().unwrap_or(Path::new("/")).to_path_buf() };
    let _ = Command::new("setsid").args(["-f", "aura-files-rs"]).arg(dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

fn copy(s: &str) {
    let _ = Command::new("wl-copy").arg("--").arg(s).stdin(Stdio::null()).status();
}

fn run(cmd: &str, args: &[String]) -> bool {
    Command::new(cmd).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        let data = Arc::new(Mutex::new(Snapshot::default()));
        let s = load_settings();
        let settings = Arc::new(Mutex::new(s.clone()));
        let (d, st, ctx) = (data.clone(), settings.clone(), cc.egui_ctx.clone());
        std::thread::spawn(move || sampler(d, st, ctx));
        Self { data, settings, edit: s, sort: Sort::Mem, tab: Tab::Procs, filter: String::new(), status: String::new(), dragging_split: false, theme_id, next_theme_check: 0.0, mem_open: false }
    }

    fn signal(&mut self, p: &Proc, sig: &str, done: &str) {
        let ok = run("kill", &[format!("-{sig}"), p.pid.to_string()]);
        self.status = if ok { format!("{done} {} (pid {})", p.name, p.pid) } else { format!("couldn't — {} belongs to another user", p.name) };
    }

    fn renice(&mut self, p: &Proc, n: i32) {
        let args = vec!["-n".to_string(), n.to_string(), "-p".to_string(), p.pid.to_string()];
        // raising priority (negative nice) needs admin: ask through pkexec
        let ok = run("renice", &args) || (n < p.nice && run("pkexec", &[vec!["renice".to_string()], args].concat()));
        self.status = if ok { format!("{} priority → nice {n}", p.name) } else { format!("couldn't change priority of {}", p.name) };
    }

    fn proc_menu(&mut self, ui: &mut Ui, p: &Proc) {
        ui.label(RichText::new(format!("{}  ·  pid {}", p.name, p.pid)).color(theme::accent()));
        ui.separator();
        let exe = std::fs::read_link(format!("/proc/{}/exe", p.pid)).ok();
        if ui.add_enabled(exe.is_some(), egui::Button::new("open its folder in Files")).clicked() {
            if let Some(e) = &exe {
                open_folder(e);
                self.status = format!("opened {}", e.parent().unwrap_or(Path::new("/")).display());
            }
            ui.close_menu();
        }
        if ui.add_enabled(exe.is_some(), egui::Button::new("copy program path")).clicked() {
            if let Some(e) = &exe {
                copy(&e.to_string_lossy());
                self.status = "path copied".into();
            }
            ui.close_menu();
        }
        ui.separator();
        if ui.button("end  (ask it to quit)").clicked() {
            self.signal(p, "TERM", "asked to quit:");
            ui.close_menu();
        }
        if ui.button("kill  (force, now)").clicked() {
            self.signal(p, "KILL", "killed");
            ui.close_menu();
        }
        if p.state == 'T' {
            if ui.button("continue").clicked() {
                self.signal(p, "CONT", "continued");
                ui.close_menu();
            }
        } else if ui.button("pause  (stop)").clicked() {
            self.signal(p, "STOP", "paused");
            ui.close_menu();
        }
        ui.menu_button(format!("priority  ·  now nice {}", p.nice), |ui| {
            for (label, n) in [("highest  (−10, asks admin)", -10), ("high  (−5, asks admin)", -5), ("normal  (0)", 0), ("low  (10)", 10), ("lowest  (19)", 19)] {
                if ui.selectable_label(p.nice == n, label).clicked() {
                    self.renice(p, n);
                    ui.close_menu();
                }
            }
        });
    }

    fn left(&mut self, ui: &mut Ui, s: &Snapshot) {
        let last = |q: &VecDeque<f32>| q.back().copied().unwrap_or(0.0);
        let sen = &s.sensors;

        // CPU: big graph + one meter per core
        let cpu = last(&s.cpu);
        let mut right = pct(cpu);
        if let Some(m) = s.mhz {
            right += &format!("  ·  {:.1} GHz", m / 1000.0);
        }
        if let Some(t) = sen.cpu {
            right += &format!("  ·  {t:.0} °C");
        }
        let cores_rows = s.cores.len().div_ceil(2) as f32;
        let hs = self.edit.heights;
        let r = panel(ui, "cpu", &right, hs[0] + 40.0 + cores_rows * 16.0);
        let g = Rect::from_min_size(r.min, vec2(r.width(), hs[0]));
        btop_graph(ui, g, &s.cpu, heat(), false);
        let half = (r.width() - 16.0) / 2.0;
        for (i, c) in s.cores.iter().enumerate() {
            let col = (i % 2) as f32;
            let row = (i / 2) as f32;
            let o = pos2(r.left() + col * (half + 16.0), g.bottom() + 10.0 + row * 16.0);
            let v = last(c);
            text(ui, o, Align2::LEFT_TOP, &format!("c{i:<2}"), theme::subtext0());
            meter(ui, Rect::from_min_size(o + vec2(34.0, 3.0), vec2(half - 80.0, 8.0)), v);
            text(ui, pos2(o.x + half, o.y), Align2::RIGHT_TOP, &pct(v), theme::text());
        }

        if splitter(ui, "h0", &mut self.edit.heights[0], MIN_H[0]) {
            save_settings(&self.edit);
        }
        // Memory: stacked bar by type, then the list
        let mi = &s.mem_info;
        let kb = |k: &str| mi.get(k).copied().unwrap_or(0) * 1024;
        let total = kb("MemTotal").max(1);
        let segs = s.mem_segs;
        let free = kb("MemFree");
        let cache_total: u64 = segs[1..8].iter().sum();
        let used = total.saturating_sub(kb("MemAvailable"));
        // collapsed tree shrinks the whole panel: the rows' height is given
        // back and everything below moves up
        let rows_h = 17.0 * (SEGS as f32 + 1.0);
        let mem_h = 290.0 + hs[1] - if self.mem_open { 0.0 } else { rows_h };
        let r = panel(ui, "memory", &format!("{} used of {}  ·  {}", size(used), size(total), pct(used as f32 / total as f32)), mem_h);
        // one bar, every segment in its color, free last
        let bar = Rect::from_min_size(r.min, vec2(r.width(), 14.0));
        let mut x = bar.left();
        for (i, b) in segs.iter().enumerate().chain(std::iter::once((SEGS, &free))) {
            let w = bar.width() * *b as f32 / total as f32;
            let c = if i == SEGS { theme::surface1() } else { seg_meta(i).1 };
            ui.painter().rect_filled(Rect::from_min_size(pos2(x, bar.top()), vec2(w, bar.height())), 0.0, c);
            x += w;
        }
        let mut y = bar.bottom() + 8.0;
        // the breakdown is a collapsible tree: click the header to fold the
        // per-kind rows away and let the ram/swap graph take the space
        let arrow = if self.mem_open { "▾" } else { "▸" };
        let hdr = Rect::from_min_size(pos2(r.left(), y), vec2(r.width(), 16.0));
        if ui.interact(hdr, ui.id().with("mem-tree"), Sense::click()).clicked() {
            self.mem_open = !self.mem_open;
        }
        text(ui, hdr.min, Align2::LEFT_TOP, &format!("{arrow} breakdown"), theme::text());
        text(ui, pos2(hdr.right(), y), Align2::RIGHT_TOP, &format!("{} segments", SEGS + 1), theme::subtext0());
        y += 17.0;
        let mut row = |ui: &mut Ui, name: &str, b: u64, c: Option<Color32>, hint: &str, indent: f32, strong: bool| {
            let rr = Rect::from_min_size(pos2(r.left(), y), vec2(r.width(), 16.0));
            if let Some(c) = c {
                ui.painter().rect_filled(Rect::from_min_size(rr.min + vec2(indent, 4.0), vec2(8.0, 8.0)), 0.0, c);
            }
            let tc = if strong { theme::text() } else { theme::subtext0() };
            text(ui, rr.min + vec2(indent + 14.0, 0.0), Align2::LEFT_TOP, name, tc);
            text(ui, pos2(rr.right() - 60.0, y), Align2::RIGHT_TOP, &size(b), tc);
            text(ui, pos2(rr.right(), y), Align2::RIGHT_TOP, &pct(b as f32 / total as f32), theme::subtext0());
            ui.interact(rr, ui.id().with(("mem", name)), Sense::hover()).on_hover_text(hint);
            y += 17.0;
        };
        if self.mem_open {
            for i in 0..SEGS {
                if i == 1 {
                    row(ui, "file cache", cache_total, None, "files kept in RAM — split by kind below; freed on demand", 0.0, true);
                }
                let (name, c, hint, sub) = seg_meta(i);
                row(ui, name, segs[i], Some(c), hint, if sub { 16.0 } else { 0.0 }, !sub);
            }
            row(ui, "free", free, Some(theme::surface1()), "not used at all", 0.0, true);
        }
        let (st, sf) = (kb("SwapTotal"), kb("SwapFree"));
        let swap_f = if st > 0 { (st - sf) as f32 / st as f32 } else { 0.0 };
        text(ui, pos2(r.left(), y + 2.0), Align2::LEFT_TOP, "swap", if swap_f > ALERT { theme::bad() } else { theme::text() });
        meter(ui, Rect::from_min_size(pos2(r.left() + 110.0, y + 6.0), vec2(r.width() - 290.0, 8.0)), swap_f);
        text(ui, pos2(r.right() - 60.0, y + 2.0), Align2::RIGHT_TOP, &format!("{} / {}", size(st - sf), size(st)), theme::text());
        text(ui, pos2(r.right(), y + 2.0), Align2::RIGHT_TOP, &pct(swap_f), if swap_f > ALERT { theme::bad() } else { theme::subtext0() });
        y += 20.0;
        if let Some((u, t)) = sen.vram {
            let gtt = sen.gtt.map(|(gu, _)| format!(" + {} shared", size(gu))).unwrap_or_default();
            text(ui, pos2(r.left(), y), Align2::LEFT_TOP, "graphics", theme::text());
            meter(ui, Rect::from_min_size(pos2(r.left() + 110.0, y + 4.0), vec2(r.width() - 290.0, 8.0)), u as f32 / t.max(1) as f32);
            text(ui, pos2(r.right(), y), Align2::RIGHT_TOP, &format!("{} / {}{gtt}", size(u), size(t)), theme::subtext0());
        }
        // collapsed tree? the graph grows up into the freed rows (never past
        // the swap/graphics lines — take whichever top is lower)
        let gtop = (r.bottom() - hs[1]).max(y + 12.0);
        let mg = Rect::from_min_max(pos2(r.left(), gtop), r.max);
        // ram vs swap, mirrored like the network panel: usage climbs up from
        // the middle line, swap hangs below it
        let (mtop, mbot) = mg.split_top_bottom_at_fraction(0.5);
        btop_graph(ui, mtop, &s.mem, heat(), false);
        btop_graph(ui, mbot, &s.swap, heat(), true);
        text(ui, mtop.left_top(), Align2::LEFT_TOP, &format!("ram  {} used", size(used)), theme::text());
        text(ui, mbot.left_bottom(), Align2::LEFT_BOTTOM, &format!("swap  {} / {}", size(st - sf), size(st)), if swap_f > ALERT { theme::bad() } else { theme::text() });

        if splitter(ui, "h1", &mut self.edit.heights[1], MIN_H[1]) {
            save_settings(&self.edit);
        }
        // GPU + temperatures, laid out like the network panel: two columns,
        // each a mirrored pair — the upper graph climbs from the middle line,
        // the lower one hangs from it.
        let temp_n = |t: f32| ((t - 30.0) / 70.0).clamp(0.0, 1.0); // 30–100 °C
        ui.columns(2, |cols| {
            let gb = last(&s.gpu);
            let gt = last(&s.temp_gpu);
            let r = panel(&mut cols[0], "graphics", &format!("busy {}  ·  {:.0} °C", pct(gb), gt), hs[2]);
            let (top, bot) = r.split_top_bottom_at_fraction(0.5);
            btop_graph(&cols[0], top, &s.gpu, [theme::accent3(), theme::accent(), theme::accent2()], false);
            btop_graph(&cols[0], bot, &s.temp_gpu.iter().map(|t| temp_n(*t)).collect(), heat(), true);
            text(&cols[0], top.left_top(), Align2::LEFT_TOP, &format!("busy {}", pct(gb)), theme::text());
            text(&cols[0], bot.left_bottom(), Align2::LEFT_BOTTOM, &format!("{:.0} °C", gt), grad(heat(), temp_n(gt)));
            let ct = sen.cpu;
            let nt = last(&s.temp_nvme);
            let r = panel(&mut cols[1], "temperatures", &format!("cpu {}  ·  ssd {:.0} °C", ct.map(|t| format!("{t:.0} °C")).unwrap_or_default(), nt), hs[2]);
            let (top, bot) = r.split_top_bottom_at_fraction(0.5);
            btop_graph(&cols[1], top, &s.temp_cpu.iter().map(|t| temp_n(*t)).collect(), heat(), false);
            btop_graph(&cols[1], bot, &s.temp_nvme.iter().map(|t| temp_n(*t)).collect(), heat(), true);
            if let Some(t) = ct {
                text(&cols[1], top.left_top(), Align2::LEFT_TOP, &format!("{t:.0} °C"), grad(heat(), temp_n(t)));
            }
            text(&cols[1], bot.left_bottom(), Align2::LEFT_BOTTOM, &format!("ssd {nt:.0} °C"), grad(heat(), temp_n(nt)));
        });

        if splitter(ui, "h2", &mut self.edit.heights[2], MIN_H[2]) {
            save_settings(&self.edit);
        }
        // Network + disk, each up/down mirrored like btop
        ui.columns(2, |cols| {
            let rx_max = max_of(&s.net_rx).max(max_of(&s.net_tx)).max(64.0 * 1024.0);
            let r = panel(&mut cols[0], "network", &format!("↓ {}  ↑ {}", speed(last(&s.net_rx)), speed(last(&s.net_tx))), hs[3]);
            let (top, bot) = r.split_top_bottom_at_fraction(0.5);
            btop_graph(&cols[0], top, &s.net_rx.iter().map(|v| v / rx_max).collect(), [theme::accent3(), theme::accent(), theme::accent2()], false);
            btop_graph(&cols[0], bot, &s.net_tx.iter().map(|v| v / rx_max).collect(), [theme::accent3(), theme::accent(), theme::accent2()], true);
            let d_max = max_of(&s.disk_r).max(max_of(&s.disk_w)).max(1024.0 * 1024.0);
            let df = s.disk_used as f32 / s.disk_total.max(1) as f32;
            let r = panel(&mut cols[1], "disk", &format!("{} used · {}", pct(df), size(s.disk_total)), hs[3]);
            let (top, bot) = r.split_top_bottom_at_fraction(0.5);
            btop_graph(&cols[1], top, &s.disk_r.iter().map(|v| v / d_max).collect(), [theme::ok(), theme::warn(), theme::bad()], false);
            btop_graph(&cols[1], bot, &s.disk_w.iter().map(|v| v / d_max).collect(), [theme::ok(), theme::warn(), theme::bad()], true);
            text(&cols[1], top.left_top(), Align2::LEFT_TOP, &format!("read {}", speed(last(&s.disk_r))), theme::text());
            text(&cols[1], bot.left_bottom(), Align2::LEFT_BOTTOM, &format!("write {}", speed(last(&s.disk_w))), theme::text());
        });

        if splitter(ui, "h3", &mut self.edit.heights[3], MIN_H[3]) {
            save_settings(&self.edit);
        }
        // Electricity
        let w = last(&s.watts);
        let src = if s.on_battery { "battery reading" } else { "chip reading + other parts" };
        let r = panel(ui, "electricity", &format!("{w:.1} W  ·  {src}"), hs[4]);
        let g = Rect::from_min_size(r.min, vec2(r.width() * 0.45, r.height()));
        let wmax = max_of(&s.watts).max(30.0);
        btop_graph(ui, g, &s.watts.iter().map(|v| v / wmax).collect(), heat(), false);
        let price = self.edit.price;
        let cur = self.edit.currency.clone();
        let mins = s.started.map(|t| t.elapsed().as_secs_f64() / 60.0).unwrap_or(0.0);
        let avg_w = if mins > 0.0 { s.wh / (mins / 60.0) } else { w as f64 };
        let lines = [
            ("now, per hour", format!("{:.3} {cur}", w as f64 / 1000.0 * price)),
            ("a full day", format!("{:.2} {cur}", avg_w * 24.0 / 1000.0 * price)),
            ("a month (8 h/day)", format!("{:.2} {cur}", avg_w * 8.0 * 30.0 / 1000.0 * price)),
            (&*format!("since opened ({:.0} min)", mins), format!("{:.1} Wh · {:.4} {cur}", s.wh, s.wh / 1000.0 * price)),
        ];
        let x = g.right() + 14.0;
        for (i, (k, v)) in lines.iter().enumerate() {
            let y = r.top() + i as f32 * 17.0;
            text(ui, pos2(x, y), Align2::LEFT_TOP, k, theme::subtext0());
            text(ui, pos2(r.right(), y), Align2::RIGHT_TOP, v, theme::text());
        }
        // editable price row
        let row = Rect::from_min_max(pos2(x, r.top() + 72.0), r.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.label(RichText::new("price / kWh").size(12.0).color(theme::subtext0()));
        child.add(egui::DragValue::new(&mut self.edit.price).speed(0.005).range(0.0..=10.0).max_decimals(3));
        child.add(egui::TextEdit::singleline(&mut self.edit.currency).desired_width(28.0));
        child.label(RichText::new("+ other").size(12.0).color(theme::subtext0()))
            .on_hover_text("watts the chip's reading can't see — screen, SSD, wifi. Used when plugged in; on battery the battery reports the whole laptop.");
        child.add(egui::DragValue::new(&mut self.edit.extra_w).speed(0.5).range(0.0..=200.0).suffix(" W"));
        if splitter(ui, "h4", &mut self.edit.heights[4], MIN_H[4]) {
            save_settings(&self.edit);
        }
        if *self.settings.lock().unwrap() != self.edit {
            *self.settings.lock().unwrap() = self.edit.clone();
            save_settings(&self.edit);
        }
    }

    fn right(&mut self, ui: &mut Ui, s: &Snapshot) {
        ui.horizontal(|ui| {
            if ui.selectable_label(self.tab == Tab::Procs, format!("processes {}", s.procs.len())).clicked() {
                self.tab = Tab::Procs;
            }
            let models = s.files.iter().filter(|f| f.model).count();
            let label = if models > 0 { format!("files · {models} AI") } else { "files".into() };
            if ui.selectable_label(self.tab == Tab::Files, label).clicked() {
                self.tab = Tab::Files;
            }
        });
        // own row, and it never asks for more width than the pane has
        ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("filter").desired_width(ui.available_width()).min_size(vec2(0.0, 0.0)));
        widgets::hairline(ui);
        let f = self.filter.to_lowercase();
        match self.tab {
            Tab::Procs => {
                let mut procs: Vec<&Proc> = s.procs.iter().filter(|p| f.is_empty() || p.name.to_lowercase().contains(&f) || p.pid.to_string() == f).collect();
                match self.sort {
                    Sort::Mem => {}
                    Sort::Cpu => procs.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
                    Sort::Name => procs.sort_by_key(|p| p.name.to_lowercase()),
                    Sort::Pid => procs.sort_by_key(|p| p.pid),
                }
                // columns anchored to the right edge, so the name gets what's left
                let w = ui.available_width();
                // narrow pane: drop pid, state and nice; keep name, cpu, memory
                let compact = w < 460.0;
                let x0 = if compact { -64.0 } else { 0.0 };
                let (x_cpu, x_mem) = if compact { (w - 190.0, w - 70.0) } else { (w - 270.0, w - 150.0) };
                let cols = [(Sort::Pid, "pid", if compact { -999.0 } else { 0.0 }), (Sort::Name, "program", 64.0 + x0), (Sort::Cpu, "cpu", x_cpu), (Sort::Mem, "memory", x_mem)];
                let (hr, _) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
                for (k, label, x) in cols {
                    if x < 0.0 {
                        continue; // column hidden in the narrow layout
                    }
                    let r = Rect::from_min_size(hr.min + vec2(x, 0.0), vec2(90.0, 18.0));
                    let resp = ui.interact(r, ui.id().with(label), Sense::click());
                    let c = if self.sort == k { theme::accent() } else { theme::subtext0() };
                    text(ui, r.left_center(), Align2::LEFT_CENTER, &format!("{label}{}", if self.sort == k { " ▾" } else { "" }), c);
                    if resp.clicked() {
                        self.sort = k;
                    }
                }
                if !compact {
                    text(ui, pos2(hr.right(), hr.center().y), Align2::RIGHT_CENTER, "state  nice", theme::subtext0());
                }
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for p in procs.iter().take(200) {
                        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(row, 0.0, theme::surface0());
                        }
                        let dimc = if p.mine { theme::text() } else { theme::overlay0() };
                        if !compact {
                            text(ui, row.left_center(), Align2::LEFT_CENTER, &p.pid.to_string(), theme::subtext0());
                        }
                        let fit = ((x_cpu - 72.0 - x0) / 7.5).max(4.0) as usize;
                        text(ui, row.left_center() + vec2(64.0 + x0, 0.0), Align2::LEFT_CENTER, &p.name.chars().take(fit).collect::<String>(), dimc);
                        let cf = (p.cpu / 100.0).min(1.0);
                        meter(ui, Rect::from_min_size(row.left_center() + vec2(x_cpu, -4.0), vec2(50.0, 8.0)), cf);
                        text(ui, row.left_center() + vec2(x_cpu + 95.0, 0.0), Align2::RIGHT_CENTER, &format!("{:.0}%", p.cpu), grad(heat(), cf));
                        text(ui, row.left_center() + vec2(x_mem + 70.0, 0.0), Align2::RIGHT_CENTER, &size(p.rss), theme::text());
                        if compact {
                            let p = (*p).clone();
                            resp.on_hover_text(format!("pid {} · {} threads · nice {} · right-click for actions", p.pid, p.threads, p.nice)).context_menu(|ui| self.proc_menu(ui, &p));
                            continue;
                        }
                        let st = match p.state { 'R' => "run", 'S' => "sleep", 'D' => "disk", 'T' => "paused", 'Z' => "zombie", 'I' => "idle", _ => "?" };
                        text(ui, pos2(row.right() - 40.0, row.center().y), Align2::RIGHT_CENTER, st, if p.state == 'T' { theme::warn() } else { theme::subtext0() });
                        text(ui, pos2(row.right(), row.center().y), Align2::RIGHT_CENTER, &p.nice.to_string(), theme::subtext0());
                        let p = (*p).clone();
                        resp.on_hover_text(format!("{} threads · right-click for actions", p.threads)).context_menu(|ui| self.proc_menu(ui, &p));
                    }
                });
            }
            Tab::Files => {
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for m in s.files.iter().filter(|m| f.is_empty() || m.path.to_lowercase().contains(&f)) {
                        let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(row, 0.0, theme::surface0());
                        }
                        let name = Path::new(&m.path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        let mut x = row.left() + 4.0;
                        if m.model {
                            let fresh = s.new_models.contains(&m.path);
                            let tag = if fresh { " AI model · new " } else { " AI model " };
                            let g = ui.painter().layout_no_wrap(tag.into(), FontId::monospace(11.0), theme::base());
                            let tr = Rect::from_min_size(pos2(x, row.top() + 2.0), g.size());
                            ui.painter().rect_filled(tr, 2.0, if fresh { theme::bad() } else { theme::accent() });
                            ui.painter().galley(tr.min, g, theme::base());
                            x = tr.right() + 6.0;
                        }
                        text(ui, pos2(x, row.top() + 2.0), Align2::LEFT_TOP, &name, if m.model { theme::accent() } else { theme::text() });
                        text(ui, pos2(row.right(), row.top() + 2.0), Align2::RIGHT_TOP, &format!("{} in RAM of {}", size(m.cached), size(m.size)), theme::text());
                        // kind swatch, same color as in the memory graph
                        ui.painter().rect_filled(Rect::from_min_size(pos2(row.right() - 8.0, row.top() + 20.0), vec2(8.0, 8.0)), 0.0, m.kind.color());
                        let dir = Path::new(&m.path).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                        text(ui, pos2(row.left() + 4.0, row.top() + 18.0), Align2::LEFT_TOP, &format!("{dir}  ·  {}", m.who.join(", ")), theme::overlay0());
                        let path = m.path.clone();
                        resp.on_hover_text(&m.path).context_menu(|ui| {
                            if ui.button("open its folder in Files").clicked() {
                                open_folder(Path::new(&path));
                                ui.close_menu();
                            }
                            if ui.button("copy path").clicked() {
                                copy(&path);
                                self.status = "path copied".into();
                                ui.close_menu();
                            }
                        });
                    }
                });
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        if now >= self.next_theme_check {
            let t = theme::ThemeId::detect();
            if t != self.theme_id {
                theme::apply(ctx, &t);
                self.theme_id = t;
            }
            self.next_theme_check = now + 2.0;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) && !ctx.wants_keyboard_input() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let s = self.data.lock().unwrap().clone();

        if !self.status.is_empty() {
            egui::TopBottomPanel::bottom("status")
                .frame(Frame::new().fill(theme::base()).inner_margin(Margin::symmetric(12, 4)))
                .show_separator_line(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new(&self.status).size(11.0).color(theme::subtext0()));
                });
        }
        // The divider between graphs and processes, driven here rather than
        // by egui's panel memory (which kept the width of the window's first,
        // tiny frame): an 8 px grab area, a line that lights up on hover, and
        // the width you leave it at is saved.
        let screen = ctx.screen_rect();
        let max_w = (screen.width() - 380.0).max(320.0);
        let width = self.edit.split.clamp(320.0, max_w);
        let x = screen.right() - width;
        let pointer = ctx.pointer_hover_pos();
        let near = pointer.is_some_and(|p| (p.x - x).abs() <= 8.0);
        let (pressed, down, released) = ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released()));
        if pressed && near {
            self.dragging_split = true;
        }
        if self.dragging_split && down {
            if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
                self.edit.split = (screen.right() - p.x).clamp(320.0, max_w);
            }
        }
        if released && self.dragging_split {
            self.dragging_split = false;
            *self.settings.lock().unwrap() = self.edit.clone();
            save_settings(&self.edit);
        }
        let pane = egui::SidePanel::right("procs")
            .resizable(false)
            .show_separator_line(false)
            .exact_width(self.edit.split.clamp(320.0, max_w))
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                if s.ready {
                    self.right(ui, &s);
                }
            });
        let r = pane.response.rect;
        let x = r.left();
        let hot = near || self.dragging_split;
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("split")));
        painter.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(if hot { 2.0_f32 } else { 1.0_f32 }, if hot { theme::accent() } else { theme::surface1() }));
        // a small grip in the middle so it reads as draggable
        for dy in [-8.0_f32, 0.0, 8.0] {
            painter.circle_filled(pos2(x, r.center().y + dy), 2.0, if hot { theme::accent() } else { theme::overlay0() });
        }
        if hot {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                widgets::header(ui, "system", "live · last 2½ minutes · right-click a process for actions");
                ui.add_space(8.0);
                if !s.ready {
                    widgets::caption(ui, "reading…");
                    return;
                }
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    self.left(ui, &s);
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura system")
            .with_app_id("aura-sysmon")
            .with_inner_size([1280.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Aura system", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_lines() {
        let v = cpu_jiffies("cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 10 0 0 90 0 0 0 0\nintr 1");
        assert_eq!(v, vec![(150, 1000), (10, 100)]);
    }

    #[test]
    fn io_parsers() {
        let ds = " 259 0 nvme0n1 10 0 200 0 5 0 100 0 0 0 0\n 259 1 nvme0n1p1 9 0 150 0 4 0 90 0 0 0 0\n";
        assert_eq!(disk_io(ds), (200 * 512, 100 * 512));
        let nd = "h1\nh2\n    lo: 999 0 0 0 0 0 0 0 999 0\n  wlan0: 1000 5 0 0 0 0 0 0 300 2\n";
        assert_eq!(net_io(nd), (1000, 300));
    }

    #[test]
    fn models() {
        assert!(is_model("/m/qwen3-8b-q4.gguf", 5 << 30));
        assert!(is_model("/m/model.safetensors", 1));
        assert!(!is_model("/usr/lib/libc.so.6", 2 << 20));
        assert!(!is_model("/x/small.bin", 1 << 20));
        assert!(is_model("/x/pytorch_model.bin", 1 << 30));
    }

    #[test]
    fn reads_this_machine() {
        assert!(cpu_jiffies(&read("/proc/stat")).len() > 1);
        let raw = procs_raw();
        assert!(raw.contains_key(&std::process::id()));
        assert!(!files_in_memory(&raw).is_empty());
        assert!(disk_usage().1 > 1);
    }

    #[test]
    fn kinds_and_residency() {
        assert_eq!(Kind::of("/usr/lib/libc.so.6", 2 << 20), Kind::Library);
        assert_eq!(Kind::of("/m/q.gguf", 4 << 30), Kind::Model);
        assert_eq!(Kind::of("/usr/share/fonts/a.ttf", 1 << 20), Kind::Font);
        // this test binary is running, so at least part of it is in RAM
        let me = std::env::current_exe().unwrap();
        let len = std::fs::metadata(&me).unwrap().len();
        assert!(resident(&me.to_string_lossy(), len) > 0);
    }

    #[test]
    fn gradient_ends() {
        let s = [Color32::from_rgb(0, 255, 0), Color32::from_rgb(255, 255, 0), Color32::from_rgb(255, 0, 0)];
        assert_eq!(grad(s, 0.0), s[0]);
        assert_eq!(grad(s, 1.0), s[2]);
    }
}
