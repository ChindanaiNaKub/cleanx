//! Disk numbers and a strict allow-list of cleanable paths.
//! No dependency outside the standard library: `df` on Unix, `std::fs` everywhere.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct Volume {
    pub name: String,
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cache,
    Temp,
    Trash,
    Meta,
    Build,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Cache => "CACHE",
            Kind::Temp => "TEMP",
            Kind::Trash => "TRASH",
            Kind::Meta => "JUNK",
            Kind::Build => "REBUILD",
        }
    }
}

/// One value decides the visible tag and the action cleanX may take.
/// Each tag maps to exactly one action, so the label never lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Impact {
    Safe,
    Caution,
    HandsOff,
}

impl Impact {
    pub fn tag(self) -> &'static str {
        match self {
            Impact::Safe => "SAFE",
            Impact::Caution => "CAUTION",
            Impact::HandsOff => "HANDS OFF",
        }
    }

    pub fn cleanable(self) -> bool {
        matches!(self, Impact::Safe | Impact::Caution)
    }
}

/// An owning tool that cleans its own cache instead of deleting files by hand.
#[derive(Clone, Debug)]
pub struct Tool {
    pub program: &'static str,
    pub args: &'static [&'static str],
    pub privileged: bool,
}

#[derive(Clone, Debug)]
pub struct CleanTarget {
    pub path: PathBuf,
    pub label: String,
    pub detail: String,
    pub kind: Kind,
    pub impact: Impact,
    pub bytes: u64,
    /// Some means run the owning tool; None means remove the path.
    pub tool: Option<Tool>,
    pub default_on: bool,
}

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i])
}

pub fn fraction(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64) as f32
    }
}

/// `df -kP`, parsed into volumes. Text is passed in so the parser can be tested.
pub fn parse_df(text: &str) -> Vec<Volume> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 6 {
            continue;
        }
        let (Some(total_k), Some(used_k), Some(free_k)) = (
            f[1].parse::<u64>().ok(),
            f[2].parse::<u64>().ok(),
            f[3].parse::<u64>().ok(),
        ) else {
            continue;
        };
        let fs = f[0];
        let mount = f[5..].join(" ");
        if matches!(
            fs,
            "tmpfs" | "devtmpfs" | "squashfs" | "overlay" | "aufs" | "none"
        ) || ["/proc", "/sys", "/dev", "/run", "/snap"]
            .iter()
            .any(|p| mount.starts_with(p))
        {
            continue;
        }
        let name = if mount == "/" {
            "System".to_string()
        } else {
            mount
                .rsplit('/')
                .find(|s| !s.is_empty())
                .unwrap_or("Disk")
                .to_string()
        };
        out.push(Volume {
            name,
            mount,
            total: total_k.saturating_mul(1024),
            used: used_k.saturating_mul(1024),
            free: free_k.saturating_mul(1024),
        });
    }
    out.sort_by_key(|v| std::cmp::Reverse(v.total));
    out
}

pub fn volumes() -> Vec<Volume> {
    match Command::new("df").args(["-kP"]).output() {
        Ok(o) if o.status.success() => parse_df(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

/// The desktops cleanX supports. Backends are picked by Cargo, so rules are
/// picked at runtime and stay testable on any host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Macos,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            _ => Self::Linux,
        }
    }

    fn bit(self) -> u8 {
        match self {
            Self::Linux => 1,
            Self::Macos => 2,
            Self::Windows => 4,
        }
    }
}

const LINUX: u8 = 1;
const MACOS: u8 = 2;
const WINDOWS: u8 = 4;
const ALL: u8 = LINUX | MACOS | WINDOWS;
const THREE_DAYS: u64 = 3 * 24 * 60 * 60;

/// Paths offered only on some desktops. Others apply everywhere.
const RULE_PLATFORMS: &[(&str, u8)] = &[
    ("Library/Caches", MACOS),
    ("Library/Logs", MACOS),
    ("Library/Developer/Xcode/DerivedData", MACOS),
    (".Trash", MACOS),
    ("AppData/Local/Temp", WINDOWS),
    ("AppData/Local/CrashDumps", WINDOWS),
];

/// Minimum file age in a temp path. Fresh files there may still be in use.
const RULE_MIN_AGE: &[(&str, u64)] = &[("AppData/Local/Temp", THREE_DAYS)];

fn platforms_for(rel: &str) -> u8 {
    RULE_PLATFORMS
        .iter()
        .find(|(p, _)| *p == rel)
        .map(|(_, bits)| *bits)
        .unwrap_or(ALL)
}

fn min_age_for(rel: &str) -> u64 {
    RULE_MIN_AGE
        .iter()
        .find(|(p, _)| *p == rel)
        .map(|(_, age)| *age)
        .unwrap_or(0)
}

/// Paths that are throwaway by construction. Missing ones are ignored.
/// Fields: relative path, kind, label, detail, impact, default selection.
/// The last field is the default selection in the Clean tab.
const RULES: &[(&str, Kind, &str, &str, Impact, bool)] = &[
    (
        ".cache/thumbnails",
        Kind::Cache,
        "Thumbnails",
        "Regenerates when you browse",
        Impact::Safe,
        true,
    ),
    (
        ".cache",
        Kind::Cache,
        "App cache",
        "Downloads and app data can be fetched again",
        Impact::Safe,
        false,
    ),
    (
        "Library/Caches",
        Kind::Cache,
        "App cache",
        "macOS application caches",
        Impact::Safe,
        false,
    ),
    (
        "AppData/Local/Temp",
        Kind::Temp,
        "App temp",
        "Temporary files from installed apps",
        Impact::Safe,
        true,
    ),
    (
        ".npm/_cacache",
        Kind::Cache,
        "npm cache",
        "Restored by npm on the next install",
        Impact::Safe,
        false,
    ),
    (
        ".cargo/registry/cache",
        Kind::Cache,
        "Cargo cache",
        "Crate archives, re-downloadable",
        Impact::Safe,
        false,
    ),
    (
        ".gradle/caches",
        Kind::Cache,
        "Gradle cache",
        "Rebuilt on the next Gradle run",
        Impact::Safe,
        false,
    ),
    (
        ".local/share/Trash",
        Kind::Trash,
        "Trash",
        "Files already deleted by you",
        Impact::Safe,
        false,
    ),
    (
        ".Trash",
        Kind::Trash,
        "Trash",
        "Files already deleted by you",
        Impact::Safe,
        false,
    ),
    (
        "Library/Logs",
        Kind::Temp,
        "App logs",
        "Old diagnostic logs",
        Impact::Safe,
        true,
    ),
    (
        ".npm/_npx",
        Kind::Cache,
        "npm npx cache",
        "Rebuilt by npx on demand",
        Impact::Safe,
        false,
    ),
    (
        ".bun/install/cache",
        Kind::Cache,
        "Bun cache",
        "Rebuilt by bun on the next install",
        Impact::Safe,
        false,
    ),
    (
        ".local/share/pnpm/store",
        Kind::Cache,
        "pnpm store",
        "Re-linked on the next install",
        Impact::Caution,
        false,
    ),
    (
        "anaconda3/pkgs",
        Kind::Cache,
        "Conda packages",
        "Rebuilt from Conda channels",
        Impact::Safe,
        false,
    ),
    (
        ".rustup/toolchains",
        Kind::Build,
        "Rust toolchains",
        "Reinstall with rustup",
        Impact::Caution,
        false,
    ),
    (
        "AppData/Local/CrashDumps",
        Kind::Temp,
        "Crash dumps",
        "Reports written when an app crashed",
        Impact::Safe,
        false,
    ),
    (
        "Library/Developer/Xcode/DerivedData",
        Kind::Build,
        "Xcode build output",
        "Rebuilt by Xcode on the next build",
        Impact::Caution,
        false,
    ),
];

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn dir_size_with(path: &Path, skip: Option<&str>, cutoff: Option<SystemTime>) -> u64 {
    let mut stack = vec![path.to_path_buf()];
    let mut left = 40_000_u64;
    let mut bytes = 0_u64;
    while let Some(p) = stack.pop() {
        if left == 0 {
            break;
        }
        if skip.is_some_and(|name| p.file_name().is_some_and(|n| n == name)) {
            continue;
        }
        let Ok(md) = fs::symlink_metadata(&p) else {
            continue;
        };
        if md.file_type().is_symlink() {
            continue;
        }
        if md.is_dir() {
            if let Ok(rd) = fs::read_dir(&p) {
                for e in rd.flatten() {
                    stack.push(e.path());
                }
            }
        } else if md.is_file() {
            if cutoff.is_some_and(|t| md.modified().is_ok_and(|m| m > t)) {
                continue;
            }
            left -= 1;
            bytes += md.len();
        }
    }
    bytes
}

fn dir_size(path: &Path) -> u64 {
    dir_size_with(path, None, None)
}

/// Floor for reporting a large file. Raise it when the list gets noisy.
const LARGE_FILE_BYTES: u64 = 100 * 1024 * 1024;

/// Top-level home folders that hold user data or tool state. Report, never clean.
const PROTECTED_NAMES: &[&str] = &[
    ".codex", ".config", ".docker", ".gemini", ".mozilla", ".nvm", ".rustup", ".ssh", ".var",
];

/// A reported item is Caution and cleanable unless its name is protected.
fn report_target(root: &Path, path: PathBuf, name: &str, bytes: u64) -> CleanTarget {
    let detail = path
        .strip_prefix(root)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    CleanTarget {
        path,
        label: name.to_string(),
        detail,
        kind: Kind::Meta,
        impact: if PROTECTED_NAMES.contains(&name) {
            Impact::HandsOff
        } else {
            Impact::Caution
        },
        bytes,
        tool: None,
        default_on: false,
    }
}

/// One bounded walk finds known junk and reports other large or hidden items.
fn discover(root: &Path, known: &HashSet<PathBuf>, out: &mut Vec<CleanTarget>) {
    let private = home_dir().join(".local/share/cleanx");
    let mut stack = vec![(root.to_path_buf(), 0_u32)];
    let mut left = 30_000_u32;
    while let Some((p, depth)) = stack.pop() {
        if left == 0 {
            return;
        }
        left -= 1;
        if p.starts_with(&private) {
            continue;
        }
        // A folder holding an allow-listed path is already covered by that rule.
        if p != root && known.iter().any(|k| k.starts_with(&p) && k != &p) {
            continue;
        }
        let Ok(md) = fs::symlink_metadata(&p) else {
            continue;
        };
        if md.file_type().is_symlink() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == ".DS_Store" && md.is_file() {
            out.push(CleanTarget {
                path: p,
                label: ".DS_Store".into(),
                detail: "Finder folder settings".into(),
                kind: Kind::Meta,
                impact: Impact::Safe,
                bytes: md.len(),
                tool: None,
                default_on: true,
            });
            continue;
        }
        if name == "node_modules" && md.is_dir() {
            out.push(CleanTarget {
                path: p.clone(),
                label: "node_modules".into(),
                detail: "Reinstall with your package manager".into(),
                kind: Kind::Build,
                impact: Impact::Caution,
                bytes: dir_size(&p),
                tool: None,
                default_on: false,
            });
            continue;
        }
        if !known.contains(&p)
            && (name.starts_with('.') || (md.is_file() && md.len() >= LARGE_FILE_BYTES))
        {
            let bytes = if md.is_dir() { dir_size(&p) } else { md.len() };
            out.push(report_target(root, p.clone(), name, bytes));
        }
        if md.is_dir()
            && depth < 6
            && let Ok(rd) = fs::read_dir(&p)
        {
            // LIFO stack. Push known junk and hidden names last so they are seen first.
            let mut children: Vec<_> = rd.flatten().map(|e| (e.path(), depth + 1)).collect();
            children.sort_by_key(|(child, _)| {
                child
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.') || n == "node_modules")
            });
            stack.extend(children);
        }
    }
}

fn tool_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

fn parse_bytes_token(s: &str) -> Option<u64> {
    let cut = s.char_indices().next_back()?.0;
    let (num, unit) = s.split_at(cut);
    const K: f64 = 1024.0;
    let mult = match unit.to_ascii_lowercase().as_str() {
        "b" => 1.0,
        "k" => K,
        "m" => K * K,
        "g" => K * K * K,
        "t" => K * K * K * K,
        _ => return None,
    };
    let n: f64 = num.parse().ok()?;
    Some((n * mult) as u64)
}

/// Reads the size from `journalctl --disk-usage`, e.g. "take up 1.2G in the file system".
pub fn parse_journal_usage(text: &str) -> u64 {
    text.split_whitespace()
        .find_map(parse_bytes_token)
        .unwrap_or(0)
}

fn journal_bytes() -> u64 {
    match Command::new("journalctl").arg("--disk-usage").output() {
        Ok(o) if o.status.success() => parse_journal_usage(&String::from_utf8_lossy(&o.stdout)),
        _ => 0,
    }
}

/// System targets. `has` is injected so the list can be tested without host tools.
pub fn tool_targets_with(has: impl Fn(&str) -> bool) -> Vec<CleanTarget> {
    let mut out = Vec::new();
    if has("dnf") {
        out.push(CleanTarget {
            path: PathBuf::from("/var/cache/dnf"),
            label: "dnf package cache".into(),
            detail: "Cleaned with dnf clean packages".into(),
            kind: Kind::Cache,
            impact: Impact::Safe,
            bytes: dir_size(Path::new("/var/cache/dnf")),
            tool: Some(Tool {
                program: "dnf",
                args: &["clean", "packages"],
                privileged: true,
            }),
            default_on: true,
        });
    }
    if has("journalctl") {
        out.push(CleanTarget {
            path: PathBuf::from("/var/log/journal"),
            label: "Journal logs".into(),
            detail: "Trimmed to 200M with journalctl".into(),
            kind: Kind::Temp,
            impact: Impact::Safe,
            bytes: journal_bytes(),
            tool: Some(Tool {
                program: "journalctl",
                args: &["--vacuum-size=200M"],
                privileged: true,
            }),
            default_on: false,
        });
    }
    if has("flatpak") {
        let root = home_dir().join(".local/share/flatpak");
        out.push(CleanTarget {
            bytes: dir_size(&root),
            path: root,
            label: "Flatpak unused runtimes".into(),
            detail: "Estimated from local Flatpak data".into(),
            kind: Kind::Cache,
            impact: Impact::Safe,
            tool: Some(Tool {
                program: "flatpak",
                args: &["uninstall", "--unused", "--noninteractive", "-y"],
                privileged: false,
            }),
            default_on: true,
        });
    }
    out
}

fn tool_targets() -> Vec<CleanTarget> {
    tool_targets_with(tool_exists)
}

/// Runs the owning tool. Privileged tools go through pkexec, never through a shell.
pub fn run_tool(tool: &Tool) -> io::Result<()> {
    let status = if tool.privileged {
        Command::new("pkexec")
            .arg(tool.program)
            .args(tool.args)
            .status()?
    } else {
        Command::new(tool.program).args(tool.args).status()?
    };
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("cleaning tool failed"))
    }
}

/// A plain command the user can copy into a terminal if pkexec is unavailable.
pub fn command_line(tool: &Tool) -> String {
    let mut parts = vec![tool.program.to_string()];
    parts.extend(tool.args.iter().map(|a| a.to_string()));
    parts.join(" ")
}

/// Appends to the log under the given home. Tests use this to avoid real state.
pub fn log_change_with(entry: &str, home: &Path) {
    let path = history_path_with(home);
    let _ = fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new(".")));
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{secs} {entry}");
    }
}

/// Browser roots that hold one folder per profile. Only cache folders inside a
/// profile become targets. Cookies, history, and saved passwords stay out.
const BROWSER_ROOTS: &[(&str, u8, &str)] = &[
    (".cache/google-chrome", ALL, "Chrome"),
    (".config/google-chrome", ALL, "Chrome"),
    (".cache/chromium", ALL, "Chromium"),
    (".config/chromium", ALL, "Chromium"),
    (".cache/BraveSoftware/Brave-Browser", ALL, "Brave"),
    (".config/BraveSoftware/Brave-Browser", ALL, "Brave"),
    (".mozilla/firefox", ALL, "Firefox"),
    ("Library/Application Support/Google/Chrome", MACOS, "Chrome"),
    (
        "Library/Application Support/Firefox/Profiles",
        MACOS,
        "Firefox",
    ),
    ("AppData/Local/Google/Chrome/User Data", WINDOWS, "Chrome"),
    ("AppData/Local/Microsoft/Edge/User Data", WINDOWS, "Edge"),
    (
        "AppData/Roaming/Mozilla/Firefox/Profiles",
        WINDOWS,
        "Firefox",
    ),
];

const BROWSER_CACHE_NAMES: &[&str] = &["Cache", "Code Cache", "GPUCache", "cache2"];

fn browser_cache_targets(home: &Path, platform: Platform) -> Vec<CleanTarget> {
    let mut out = Vec::new();
    for (rel, bits, browser) in BROWSER_ROOTS {
        if bits & platform.bit() == 0 {
            continue;
        }
        let Ok(rd) = fs::read_dir(home.join(rel)) else {
            continue;
        };
        for profile in rd.flatten() {
            let Ok(md) = fs::symlink_metadata(profile.path()) else {
                continue;
            };
            if !md.is_dir() || md.file_type().is_symlink() {
                continue;
            }
            let profile_name = profile.file_name().to_string_lossy().into_owned();
            for name in BROWSER_CACHE_NAMES {
                let cache = profile.path().join(name);
                let Ok(cd) = fs::symlink_metadata(&cache) else {
                    continue;
                };
                if !cd.is_dir() || cd.file_type().is_symlink() {
                    continue;
                }
                out.push(CleanTarget {
                    bytes: dir_size(&cache),
                    path: cache,
                    label: format!("{browser} cache"),
                    detail: format!("{profile_name} profile. The browser rebuilds this."),
                    kind: Kind::Cache,
                    impact: Impact::Safe,
                    tool: None,
                    default_on: false,
                });
            }
        }
    }
    out
}

/// Builds the reclaimable list for one platform. The clock is injected so age
/// rules can be tested without waiting.
pub fn clean_targets_with(home: &Path, platform: Platform, now: SystemTime) -> Vec<CleanTarget> {
    let mut out = Vec::new();
    for (rel, kind, label, detail, impact, default_on) in RULES {
        if platforms_for(rel) & platform.bit() == 0 {
            continue;
        }
        let path = home.join(rel);
        let Ok(md) = fs::symlink_metadata(&path) else {
            continue;
        };
        if md.file_type().is_symlink() {
            continue;
        }
        let skip = if *rel == ".cache" {
            Some("thumbnails")
        } else {
            None
        };
        let age = min_age_for(rel);
        let cutoff = (age > 0)
            .then(|| now.checked_sub(Duration::from_secs(age)))
            .flatten();
        let bytes = if md.is_dir() {
            dir_size_with(&path, skip, cutoff)
        } else if cutoff.is_some_and(|t| md.modified().is_ok_and(|m| m > t)) {
            0
        } else {
            md.len()
        };
        if cutoff.is_some() && bytes == 0 {
            continue;
        }
        out.push(CleanTarget {
            bytes,
            path,
            label: (*label).into(),
            detail: (*detail).into(),
            kind: *kind,
            impact: *impact,
            tool: None,
            default_on: *default_on,
        });
    }
    out.extend(browser_cache_targets(home, platform));
    if !home.as_os_str().is_empty() {
        let known: HashSet<PathBuf> = RULES
            .iter()
            .filter(|(rel, _, _, _, _, _)| platforms_for(rel) & platform.bit() != 0)
            .map(|(rel, _, _, _, _, _)| home.join(rel))
            .collect();
        discover(home, &known, &mut out);
    }
    if platform == Platform::Linux {
        out.extend(tool_targets());
    }
    out.sort_by_key(|v| std::cmp::Reverse(v.bytes));
    out
}

pub fn clean_targets() -> Vec<CleanTarget> {
    clean_targets_with(&home_dir(), Platform::current(), SystemTime::now())
}

fn under_home(path: &Path, home: &Path) -> bool {
    let lexical = !home.as_os_str().is_empty()
        && path.starts_with(home)
        && !path.components().any(|c| matches!(c, Component::ParentDir));
    lexical
        && fs::canonicalize(home).is_ok_and(|real_home| {
            fs::canonicalize(path).is_ok_and(|real| real.starts_with(real_home))
        })
}

fn refused() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "unsafe path refused")
}

fn valid_target(t: &CleanTarget, home: &Path) -> io::Result<()> {
    if !t.impact.cleanable() {
        return Err(refused());
    }
    let md = fs::symlink_metadata(&t.path)?;
    if md.file_type().is_symlink() || !under_home(&t.path, home) {
        return Err(refused());
    }
    Ok(())
}

pub fn history_path_with(home: &Path) -> PathBuf {
    home.join(".local/share/cleanx/history.log")
}

/// Size left after an owning tool ran. The journal tool reports its own usage;
/// other tools leave a cache directory we can measure directly.
pub fn current_size(t: &CleanTarget) -> u64 {
    if t.tool
        .as_ref()
        .is_some_and(|tool| tool.program == "journalctl")
    {
        match Command::new("journalctl").arg("--disk-usage").output() {
            Ok(o) if o.status.success() => parse_journal_usage(&String::from_utf8_lossy(&o.stdout)),
            _ => 0,
        }
    } else if t.tool.is_some() {
        dir_size(&t.path)
    } else {
        0
    }
}

/// Deterministic stash path. The same target always maps to the same place, so a
/// repeated clean cannot make a second copy of the item.
pub fn quarantine_path(original: &Path, home: &Path) -> io::Result<PathBuf> {
    let rel = original
        .strip_prefix(home)
        .map_err(|_| refused())?
        .components()
        .try_fold(PathBuf::new(), |mut out, c| {
            match c {
                Component::Normal(part) => out.push(part),
                _ => return Err(refused()),
            }
            Ok(out)
        })?;
    let mut dest = home.join(".local/share/cleanx/quarantine");
    dest.push(rel);
    Ok(dest)
}

/// Moves one path target into quarantine instead of deleting it.
/// A missing source with an existing stash means the move already happened.
pub fn quarantine_target(t: &CleanTarget, home: &Path) -> io::Result<PathBuf> {
    let dest = match fs::symlink_metadata(&t.path) {
        Ok(_) => {
            valid_target(t, home)?;
            quarantine_path(&t.path, home)?
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let dest = quarantine_path(&t.path, home)?;
            if dest.exists() {
                return Ok(dest);
            }
            return Err(e);
        }
        Err(e) => return Err(e),
    };
    if dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "stash already exists",
        ));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&t.path, &dest)?;
    Ok(dest)
}

/// Moves one quarantined item back. Both paths are checked before the rename.
pub fn restore_path(original: &Path, stash: &Path, home: &Path) -> io::Result<()> {
    let qroot = home.join(".local/share/cleanx/quarantine");
    let lexical = !home.as_os_str().is_empty()
        && original.starts_with(home)
        && stash.starts_with(&qroot)
        && !original
            .components()
            .any(|c| matches!(c, Component::ParentDir));
    if !lexical || fs::canonicalize(stash).is_err() || !under_home(stash, home) {
        return Err(refused());
    }
    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(stash, original)
}

/// Clears the quarantine and its manifest. Called when the user clears held items.
pub fn purge_quarantine_with(home: &Path) {
    let root = home.join(".local/share/cleanx/quarantine");
    let _ = fs::remove_dir_all(root);
}

fn manifest_path(home: &Path) -> PathBuf {
    home.join(".local/share/cleanx/quarantine/manifest.tsv")
}

/// Saves the restore data for items currently held in quarantine.
pub fn write_manifest(entries: &[(PathBuf, String, u64)], home: &Path) -> io::Result<()> {
    let path = manifest_path(home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = String::new();
    for (original, label, bytes) in entries {
        let rel = original.strip_prefix(home).map_err(|_| refused())?;
        text.push_str(&rel.to_string_lossy());
        text.push('\t');
        text.push_str(label);
        text.push('\t');
        text.push_str(&bytes.to_string());
        text.push('\n');
    }
    fs::write(path, text)
}

/// Reads the restore data left by this or an earlier session.
pub fn read_manifest(home: &Path) -> Vec<(PathBuf, String, u64)> {
    let Ok(text) = fs::read_to_string(manifest_path(home)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let rel = fields.next()?;
            let label = fields.next()?.to_string();
            let bytes = fields.next()?.parse().ok()?;
            Some((home.join(rel), label, bytes))
        })
        .collect()
}

fn parse_history_line(line: &str) -> Option<(u64, String)> {
    let mut parts = line.splitn(2, ' ');
    let secs = parts.next()?.parse::<u64>().ok()?;
    Some((secs, parts.next().unwrap_or("").to_string()))
}

/// Reads the append-only change log. Each line is "seconds text".
pub fn read_history() -> Vec<(u64, String)> {
    read_history_with(&home_dir())
}

pub fn read_history_with(home: &Path) -> Vec<(u64, String)> {
    let Ok(text) = fs::read_to_string(history_path_with(home)) else {
        return Vec::new();
    };
    text.lines().rev().filter_map(parse_history_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn df_output_becomes_a_sorted_volume_list() {
        let sample = "Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                      /dev/sda1 1000000 400000 600000 40% /\n\
                      tmpfs 500000 1000 499000 1% /run\n\
                      /dev/sdb1 2000000 1900000 100000 95% /Volumes/Backup\n";
        let v = parse_df(sample);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "Backup");
        assert_eq!(v[0].total, 2000000 * 1024);
        assert_eq!(v[1].mount, "/");
        assert_eq!(v[1].free, 600000 * 1024);
    }

    #[test]
    fn quarantine_refuses_paths_outside_home() {
        let home = Path::new("/home/someone");
        let t = CleanTarget {
            path: PathBuf::from("/etc/passwd"),
            label: "x".into(),
            detail: "x".into(),
            kind: Kind::Cache,
            impact: Impact::Safe,
            bytes: 1,
            tool: None,
            default_on: false,
        };
        assert!(quarantine_target(&t, home).is_err());
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn history_lines_are_parsed_newest_first() {
        let lines = "100 App cache 8.0 MB\n200 dnf package cache 1.0 GB\nbad line\n";
        let parsed: Vec<_> = lines.lines().rev().filter_map(parse_history_line).collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, 200);
        assert_eq!(parsed[0].1, "dnf package cache 1.0 GB");
        assert_eq!(parsed[1].0, 100);
    }

    #[test]
    fn journal_disk_usage_is_parsed() {
        assert_eq!(
            parse_journal_usage("Archived and active journals take up 1.2G in the file system."),
            1_288_490_188
        );
        assert_eq!(
            parse_journal_usage("Total logged data: 512M\n"),
            536_870_912
        );
        assert_eq!(parse_journal_usage("nothing here"), 0);
    }

    #[test]
    fn tool_targets_follow_available_tools_and_defaults() {
        let targets = tool_targets_with(|name| name == "dnf" || name == "journalctl");
        assert_eq!(targets.len(), 2);
        let dnf = &targets[0];
        assert_eq!(dnf.label, "dnf package cache");
        assert!(dnf.default_on);
        let tool = dnf.tool.as_ref().unwrap();
        assert!(tool.privileged);
        assert_eq!(command_line(tool), "dnf clean packages");
        assert!(!targets[1].default_on);
    }

    #[test]
    fn flatpak_command_does_not_wait_for_input() {
        let targets = tool_targets_with(|name| name == "flatpak");
        let cmd = command_line(targets[0].tool.as_ref().unwrap());
        assert!(cmd.contains("--noninteractive"));
        assert!(cmd.contains("-y"));
    }

    #[test]
    fn rules_are_selected_by_platform() {
        let rels = |platform: Platform| -> Vec<&str> {
            RULES
                .iter()
                .filter(|(rel, _, _, _, _, _)| platforms_for(rel) & platform.bit() != 0)
                .map(|(rel, _, _, _, _, _)| *rel)
                .collect()
        };
        let linux = rels(Platform::Linux);
        assert!(linux.contains(&".cache"));
        assert!(
            !linux
                .iter()
                .any(|r| r.starts_with("Library/") || r.starts_with("AppData/"))
        );

        let macos = rels(Platform::Macos);
        assert!(macos.contains(&"Library/Caches"));
        assert!(macos.contains(&".Trash"));
        assert!(macos.contains(&"Library/Developer/Xcode/DerivedData"));
        assert!(!macos.contains(&"AppData/Local/Temp"));

        let windows = rels(Platform::Windows);
        assert!(windows.contains(&"AppData/Local/Temp"));
        assert!(windows.contains(&"AppData/Local/CrashDumps"));
        assert!(!windows.contains(&"Library/Caches"));
        assert_eq!(min_age_for("AppData/Local/Temp"), THREE_DAYS);
        assert_eq!(min_age_for(".cache"), 0);
    }

    #[test]
    fn aged_temp_rule_counts_only_old_files() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("cleanx-age-{}-{nanos}", std::process::id()));
        let home = base.join("home");
        let temp = home.join("AppData/Local/Temp");
        fs::create_dir_all(&temp).unwrap();
        let fresh = temp.join("fresh.tmp");
        let old = temp.join("old.tmp");
        fs::write(&fresh, b"1234567890").unwrap();
        fs::write(&old, b"abcd").unwrap();
        let past = SystemTime::now() - Duration::from_secs(10 * 24 * 60 * 60);
        fs::File::open(&old).unwrap().set_modified(past).unwrap();

        let windows = clean_targets_with(&home, Platform::Windows, SystemTime::now());
        let temp_target = windows.iter().find(|t| t.label == "App temp").unwrap();
        assert_eq!(temp_target.bytes, 4);

        let linux = clean_targets_with(&home, Platform::Linux, SystemTime::now());
        assert!(linux.iter().all(|t| t.label != "App temp"));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn browser_caches_are_found_in_every_profile() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("cleanx-browser-{}-{nanos}", std::process::id()));
        let home = base.join("home");
        let root = home.join("AppData/Local/Google/Chrome/User Data");
        for profile in ["Default", "Profile 1"] {
            let cache = root.join(profile).join("Cache");
            fs::create_dir_all(&cache).unwrap();
            fs::write(cache.join("data"), b"0123456789").unwrap();
        }

        let windows = clean_targets_with(&home, Platform::Windows, SystemTime::now());
        let caches: Vec<_> = windows
            .iter()
            .filter(|t| t.label == "Chrome cache")
            .collect();
        assert_eq!(caches.len(), 2);
        assert!(caches.iter().all(|t| t.bytes == 10));
        assert!(caches.iter().all(|t| t.impact.cleanable()));
        assert!(
            caches
                .iter()
                .all(|t| !t.path.to_string_lossy().contains("Cookies"))
        );

        let linux = clean_targets_with(&home, Platform::Linux, SystemTime::now());
        assert!(linux.iter().all(|t| t.label != "Chrome cache"));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn quarantine_moves_an_item_and_undo_restores_it() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("cleanx-q-{}-{nanos}", std::process::id()));
        let home = base.join("home");
        let dir = home.join(".cache").join("demo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), b"kept").unwrap();
        let target = CleanTarget {
            path: dir.clone(),
            label: "Demo".into(),
            detail: "test".into(),
            kind: Kind::Cache,
            impact: Impact::Safe,
            bytes: 4,
            tool: None,
            default_on: true,
        };

        let stash = quarantine_target(&target, &home).unwrap();
        assert_eq!(stash, quarantine_path(&dir, &home).unwrap());
        assert!(!dir.exists());
        assert!(stash.join("a.txt").exists());
        assert_eq!(quarantine_target(&target, &home).unwrap(), stash);

        restore_path(&dir, &stash, &home).unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "kept");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn discovery_reports_hidden_and_large_items_as_caution() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("cleanx-d-{}-{nanos}", std::process::id()));
        let home = base.join("home");
        fs::create_dir_all(home.join(".config")).unwrap();
        fs::write(home.join(".config").join("settings"), b"x").unwrap();
        fs::create_dir_all(home.join("work")).unwrap();
        let big = home.join("work").join("big.bin");
        fs::File::create(&big)
            .unwrap()
            .set_len(LARGE_FILE_BYTES + 1)
            .unwrap();

        let mut out = Vec::new();
        discover(&home, &HashSet::new(), &mut out);

        let config = out.iter().find(|t| t.label == ".config").unwrap();
        assert_eq!(config.impact, Impact::HandsOff);
        assert_eq!(config.impact.tag(), "HANDS OFF");
        assert!(!config.impact.cleanable());
        assert!(quarantine_target(config, &home).is_err());

        let large = out.iter().find(|t| t.label == "big.bin").unwrap();
        assert_eq!(large.impact, Impact::Caution);
        assert_eq!(large.impact.tag(), "CAUTION");
        assert!(large.impact.cleanable());
        assert!(large.bytes >= LARGE_FILE_BYTES);
        fs::remove_dir_all(base).unwrap();
    }
}
