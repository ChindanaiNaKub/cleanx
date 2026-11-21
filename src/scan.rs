//! Disk numbers and a strict allow-list of cleanable paths.
//! No dependency outside the standard library: `df` on Unix, `std::fs` everywhere.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Impact {
    Safe,
    Caution,
    ViewOnly,
    HandsOff,
}

impl Impact {
    pub fn tag(self) -> &'static str {
        match self {
            Impact::Safe => "SAFE",
            Impact::Caution | Impact::ViewOnly => "CAUTION",
            Impact::HandsOff => "HANDS OFF",
        }
    }

    pub fn cleanable(self) -> bool {
        matches!(self, Impact::Safe | Impact::Caution)
    }

    pub fn view_only(self) -> bool {
        matches!(self, Impact::ViewOnly | Impact::HandsOff)
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

/// Paths that are throwaway by construction. Missing ones are ignored.
/// The last field is the default selection in the Clean tab.
const RULES: &[(&str, Kind, &str, &str, bool)] = &[
    (
        ".cache/thumbnails",
        Kind::Cache,
        "Thumbnails",
        "Regenerates when you browse",
        true,
    ),
    (
        ".cache",
        Kind::Cache,
        "App cache",
        "Downloads and app data can be fetched again",
        false,
    ),
    (
        "Library/Caches",
        Kind::Cache,
        "App cache",
        "macOS application caches",
        false,
    ),
    (
        "AppData/Local/Temp",
        Kind::Temp,
        "App temp",
        "Temporary files from installed apps",
        true,
    ),
    (
        ".npm/_cacache",
        Kind::Cache,
        "npm cache",
        "Restored by npm on the next install",
        false,
    ),
    (
        ".cargo/registry/cache",
        Kind::Cache,
        "Cargo cache",
        "Crate archives, re-downloadable",
        false,
    ),
    (
        ".gradle/caches",
        Kind::Cache,
        "Gradle cache",
        "Rebuilt on the next Gradle run",
        false,
    ),
    (
        ".local/share/Trash",
        Kind::Trash,
        "Trash",
        "Files already deleted by you",
        false,
    ),
    (
        ".Trash",
        Kind::Trash,
        "Trash",
        "Files already deleted by you",
        false,
    ),
    (
        "Library/Logs",
        Kind::Temp,
        "App logs",
        "Old diagnostic logs",
        true,
    ),
];

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn dir_size_skip(path: &Path, skip: Option<&str>) -> u64 {
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
            left -= 1;
            bytes += md.len();
        }
    }
    bytes
}

fn dir_size(path: &Path) -> u64 {
    dir_size_skip(path, None)
}

/// Floor for reporting a large file. Raise it when the list gets noisy.
const LARGE_FILE_BYTES: u64 = 100 * 1024 * 1024;

fn view_target(root: &Path, path: PathBuf, name: &str, bytes: u64, hands_off: bool) -> CleanTarget {
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
        impact: if hands_off {
            Impact::HandsOff
        } else {
            Impact::ViewOnly
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
            let hands_off = name == ".config" || name == ".ssh" || name.starts_with(".local");
            let bytes = if md.is_dir() { dir_size(&p) } else { md.len() };
            out.push(view_target(root, p.clone(), name, bytes, hands_off));
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

/// Appends one line to the local change log. Never fails the clean if logging fails.
pub fn log_change(entry: &str) {
    log_change_with(entry, &home_dir());
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

pub fn clean_targets() -> Vec<CleanTarget> {
    let home = home_dir();
    let mut out = Vec::new();
    for (rel, kind, label, detail, default_on) in RULES {
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
        out.push(CleanTarget {
            bytes: if md.is_dir() {
                dir_size_skip(&path, skip)
            } else {
                md.len()
            },
            path,
            label: (*label).into(),
            detail: (*detail).into(),
            kind: *kind,
            impact: Impact::Safe,
            tool: None,
            default_on: *default_on,
        });
    }
    if !home.as_os_str().is_empty() {
        let known: HashSet<PathBuf> = RULES
            .iter()
            .map(|(rel, _, _, _, _)| home.join(rel))
            .collect();
        discover(&home, &known, &mut out);
    }
    out.extend(tool_targets());
    out.sort_by_key(|v| std::cmp::Reverse(v.bytes));
    out
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

pub fn history_path() -> PathBuf {
    history_path_with(&home_dir())
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

/// Clears the quarantine left by a previous session. Called once at app startup.
pub fn purge_quarantine() {
    let root = home_dir().join(".local/share/cleanx/quarantine");
    let _ = fs::remove_dir_all(root);
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
    fn discovery_marks_hidden_and_large_items_view_only() {
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
        assert_eq!(large.impact, Impact::ViewOnly);
        assert_eq!(large.impact.tag(), "CAUTION");
        assert!(!large.impact.cleanable());
        assert!(large.bytes >= LARGE_FILE_BYTES);
        fs::remove_dir_all(base).unwrap();
    }
}
