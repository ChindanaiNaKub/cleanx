//! One owner for everything that changes during a cleanX session.
//! Path guards stay in `scan.rs`. This module decides which items to act on.

use std::collections::HashSet;
use std::f64::consts::{PI, TAU};
use std::path::PathBuf;
use std::time::Duration;

use rune::Tx;

use crate::scan;

const SCAN_SECS: f64 = 0.9;
const BURST_SECS: f64 = 1.6;
/// Radians per second. The ring settles at rest and spins on Scan or drag.
const SPIN_IDLE: f64 = 0.0;
const SPIN_SCAN: f64 = 2.2;

#[derive(Clone, Debug)]
pub struct Candidate {
    pub target: scan::CleanTarget,
    pub chosen: bool,
    /// Tool targets never enter quarantine, so the UI cannot offer undo for them.
    pub undoable: bool,
}

#[derive(Clone, Debug)]
pub struct Quarantined {
    pub target: scan::CleanTarget,
    pub stash: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanPhase {
    Idle,
    Armed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ScanPhase {
    Idle,
    Requested,
    Running { start: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Burst {
    None,
    Requested,
    At(f64),
}

#[derive(Clone, Debug, Default)]
pub struct CleanReport {
    pub path_items: usize,
    pub tool_items: usize,
    pub failed: usize,
    pub retry: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct UndoReport {
    pub restored: usize,
    pub failed: usize,
    pub reason: Option<String>,
}

pub struct Session {
    volumes: Vec<scan::Volume>,
    volume: usize,
    candidates: Vec<Candidate>,
    clean_phase: CleanPhase,
    quarantine: Vec<Quarantined>,
    freed_total: u64,
    history: Vec<(u64, String)>,
    scan: ScanPhase,
    scan_progress: f32,
    burst: Burst,
    spin_phase: f64,
    spin_velocity: f64,
    spin_drag: Option<f64>,
    last_tick: Option<f64>,
}

impl Session {
    pub fn new() -> Self {
        let mut s = Self {
            volumes: scan::volumes(),
            volume: 0,
            candidates: Vec::new(),
            clean_phase: CleanPhase::Idle,
            quarantine: Vec::new(),
            freed_total: 0,
            history: scan::read_history(),
            scan: ScanPhase::Idle,
            scan_progress: 1.0,
            burst: Burst::None,
            spin_phase: 0.0,
            spin_velocity: SPIN_IDLE,

            spin_drag: None,
            last_tick: None,
        };
        s.reload_candidates();
        s
    }

    /// Called once by the app, not by tests or snapshot scenes. Held items survive restarts.
    pub fn load_quarantine(&mut self) {
        self.load_quarantine_with_home(&scan::home_dir());
    }

    pub fn load_quarantine_with_home(&mut self, home: &std::path::Path) {
        self.quarantine = scan::read_manifest(home)
            .into_iter()
            .filter_map(|(path, label, bytes)| {
                let stash = scan::quarantine_path(&path, home).ok()?;
                if !stash.exists() {
                    return None;
                }
                Some(Quarantined {
                    target: scan::CleanTarget {
                        path,
                        label,
                        detail: "Held from an earlier session".into(),
                        kind: scan::Kind::Meta,
                        impact: scan::Impact::Caution,
                        bytes,
                        tool: None,
                        default_on: false,
                    },
                    stash,
                })
            })
            .collect();
    }

    fn reload_candidates(&mut self) {
        self.set_candidates(scan::clean_targets());
    }

    pub fn set_candidates(&mut self, targets: Vec<scan::CleanTarget>) {
        self.candidates = targets
            .into_iter()
            .map(|t| Candidate {
                chosen: t.default_on && t.impact.cleanable(),
                undoable: t.tool.is_none(),
                target: t,
            })
            .collect();
    }

    pub fn volumes(&self) -> &[scan::Volume] {
        &self.volumes
    }

    pub fn volume_index(&self) -> usize {
        self.volume
    }

    pub fn current_volume(&self) -> Option<&scan::Volume> {
        self.volumes
            .get(self.volume)
            .or_else(|| self.volumes.first())
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    pub fn chosen_count(&self) -> usize {
        self.candidates.iter().filter(|c| c.chosen).count()
    }

    pub fn reclaimable(&self) -> u64 {
        self.candidates
            .iter()
            .filter(|c| c.chosen)
            .map(|c| c.target.bytes)
            .sum()
    }

    pub fn armed(&self) -> bool {
        self.clean_phase == CleanPhase::Armed
    }

    pub fn quarantine(&self) -> &[Quarantined] {
        &self.quarantine
    }

    pub fn history(&self) -> &[(u64, String)] {
        &self.history
    }

    /// Still on disk in quarantine. These bytes return at the next start.
    pub fn held_bytes(&self) -> u64 {
        self.quarantine.iter().map(|q| q.target.bytes).sum::<u64>()
    }

    /// Returned to the volume by owning tools and by clearing held items.
    pub fn freed_bytes(&self) -> u64 {
        self.freed_total
    }

    pub fn scan_progress(&self) -> f32 {
        self.scan_progress
    }

    pub fn burst_time(&self) -> Option<f64> {
        match self.burst {
            Burst::At(t) => Some(t),
            _ => None,
        }
    }

    /// Angular offset for the sunburst geometry, in radians.
    pub fn spin_phase(&self) -> f32 {
        self.spin_phase as f32
    }

    pub fn spin_drag_start(&mut self, angle: f32) {
        self.spin_drag = Some(angle as f64);
        self.spin_velocity = 0.0;
    }

    pub fn spin_drag_move(&mut self, angle: f32) {
        let Some(previous) = self.spin_drag else {
            return;
        };
        let angle = angle as f64;
        let mut delta = angle - previous;
        if delta > PI {
            delta -= TAU;
        } else if delta < -PI {
            delta += TAU;
        }
        self.spin_phase += delta;
        self.spin_velocity = delta * 18.0;
        self.spin_drag = Some(angle);
    }

    pub fn spin_drag_end(&mut self) {
        self.spin_drag = None;
    }

    pub fn select_volume(&mut self, i: usize) {
        self.volume = i;
    }

    pub fn toggle(&mut self, i: usize) {
        if self
            .candidates
            .get(i)
            .is_some_and(|c| c.target.impact.cleanable())
        {
            let c = &mut self.candidates[i];
            c.chosen = !c.chosen;
        }
        self.clean_phase = CleanPhase::Idle;
    }

    pub fn arm(&mut self) -> bool {
        if !self.candidates.iter().any(|c| c.chosen) {
            return false;
        }
        self.clean_phase = CleanPhase::Armed;
        true
    }

    pub fn scan_now(&mut self) {
        self.volumes = scan::volumes();
        self.reload_candidates();
        self.history = scan::read_history();
        self.clean_phase = CleanPhase::Idle;
        self.scan = ScanPhase::Requested;
        self.scan_progress = 0.12;
        self.spin_velocity = SPIN_SCAN;
    }

    /// First press arms. Second press stashes path items and runs owning tools.
    pub fn clean(&mut self) -> CleanReport {
        self.clean_with_home(&scan::home_dir())
    }

    /// The same clean with an injected home, so the file flow can be tested.
    pub fn clean_with_home(&mut self, home: &std::path::Path) -> CleanReport {
        if self.clean_phase != CleanPhase::Armed {
            return CleanReport::default();
        }
        self.clean_phase = CleanPhase::Idle;
        let mut jobs: Vec<usize> = self
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.chosen && c.target.impact.cleanable())
            .map(|(i, _)| i)
            .collect();
        // Parents first. A stashed parent already covers any selected child.
        jobs.sort_by_key(|&i| self.candidates[i].target.path.components().count());

        let mut report = CleanReport::default();
        let mut done: HashSet<PathBuf> = HashSet::new();
        let mut stashed_roots: Vec<PathBuf> = Vec::new();
        for i in jobs {
            let target = self.candidates[i].target.clone();
            if stashed_roots
                .iter()
                .any(|root| target.path.starts_with(root))
            {
                done.insert(target.path.clone());
                continue;
            }
            let result = match &target.tool {
                Some(tool) => scan::run_tool(tool).map(|()| None),
                None => scan::quarantine_target(&target, home).map(Some),
            };
            match result {
                Ok(stash) => {
                    let freed = if let Some(stash) = stash {
                        self.quarantine.push(Quarantined {
                            target: target.clone(),
                            stash,
                        });
                        report.path_items += 1;
                        stashed_roots.push(target.path.clone());
                        target.bytes
                    } else {
                        let left = scan::current_size(&target);
                        let freed = target.bytes.saturating_sub(left);
                        self.freed_total = self.freed_total.saturating_add(freed);
                        report.tool_items += 1;
                        freed
                    };
                    scan::log_change_with(
                        &format!("{} {}", target.label, scan::human(freed)),
                        home,
                    );
                    done.insert(target.path.clone());
                }
                Err(_) => {
                    report.failed += 1;
                    if let Some(tool) = &target.tool {
                        if report.retry.is_none() {
                            report.retry = Some(format!("sudo {}", scan::command_line(tool)));
                        }
                    }
                }
            }
        }
        self.candidates.retain(|c| !done.contains(&c.target.path));
        let held: Vec<_> = self
            .quarantine
            .iter()
            .map(|q| {
                (
                    q.target.path.clone(),
                    q.target.label.clone(),
                    q.target.bytes,
                )
            })
            .collect();
        let _ = scan::write_manifest(&held, home);
        self.history = scan::read_history_with(home);
        if report.path_items + report.tool_items > 0 {
            self.burst = Burst::Requested;
        }
        report
    }

    pub fn undo(&mut self) -> UndoReport {
        self.undo_with_home(&scan::home_dir())
    }

    /// The same undo with an injected home, so the restore flow can be tested.
    pub fn undo_with_home(&mut self, home: &std::path::Path) -> UndoReport {
        self.clean_phase = CleanPhase::Idle;
        let mut report = UndoReport::default();
        let mut restored = Vec::new();
        let mut keep = Vec::new();
        for q in std::mem::take(&mut self.quarantine) {
            match scan::restore_path(&q.target.path, &q.stash, home) {
                Ok(()) => {
                    report.restored += 1;
                    scan::log_change_with(&format!("Restored {}", q.target.label), home);
                    restored.push(q.target);
                }
                Err(err) => {
                    report.failed += 1;
                    report.reason = Some(err.to_string());
                    keep.push(q);
                }
            }
        }
        self.quarantine = keep;
        let held: Vec<_> = self
            .quarantine
            .iter()
            .map(|q| {
                (
                    q.target.path.clone(),
                    q.target.label.clone(),
                    q.target.bytes,
                )
            })
            .collect();
        let _ = scan::write_manifest(&held, home);
        for target in restored {
            self.candidates.push(Candidate {
                target,
                chosen: false,
                undoable: true,
            });
        }
        self.history = scan::read_history_with(home);
        report
    }

    /// Removes held items for good and returns the bytes given back to the volume.
    pub fn clear_held(&mut self) -> u64 {
        self.clear_held_with_home(&scan::home_dir())
    }

    pub fn clear_held_with_home(&mut self, home: &std::path::Path) -> u64 {
        self.clean_phase = CleanPhase::Idle;
        let bytes = self.held_bytes();
        scan::purge_quarantine_with(home);
        self.quarantine.clear();
        if bytes > 0 {
            self.freed_total = self.freed_total.saturating_add(bytes);
            scan::log_change_with(&format!("Cleared held {}", scan::human(bytes)), home);
        }
        self.history = scan::read_history_with(home);
        bytes
    }

    /// Advances the scan and confetti clocks. Returns true only on a visible change.
    pub fn tick(&mut self, t: f64, tx: &mut Tx) -> bool {
        let mut changed = false;
        let dt = self
            .last_tick
            .map(|last| (t - last).clamp(0.0, 0.1))
            .unwrap_or(1.0 / 60.0);
        self.last_tick = Some(t);
        if self.spin_drag.is_none() {
            let target = if self.scan == ScanPhase::Idle {
                SPIN_IDLE
            } else {
                SPIN_SCAN
            };
            let ease = (dt * 3.0).clamp(0.0, 1.0);
            self.spin_velocity += (target - self.spin_velocity) * ease;
            self.spin_phase = (self.spin_phase + self.spin_velocity * dt) % TAU;
            if self.spin_velocity.abs() > 0.02 {
                changed = true;
                tx.wake_after(Duration::from_millis(16));
            }
        }
        match self.scan {
            ScanPhase::Requested => {
                self.scan = ScanPhase::Running { start: t };
                changed = true;
            }
            ScanPhase::Running { start } => {
                let p = ((t - start) / SCAN_SECS).clamp(0.0, 1.0) as f32;
                self.scan_progress = 0.12 + 0.88 * p;
                changed = true;
                if p >= 1.0 {
                    self.scan = ScanPhase::Idle;
                }
            }
            ScanPhase::Idle => {}
        }
        if self.scan != ScanPhase::Idle {
            tx.wake_after(Duration::from_millis(16));
        }
        match self.burst {
            Burst::Requested => {
                self.burst = Burst::At(t);
                changed = true;
                tx.wake_after(Duration::from_millis(16));
            }
            Burst::At(at) if t - at >= BURST_SECS => {
                self.burst = Burst::None;
                changed = true;
            }
            Burst::At(_) => tx.wake_after(Duration::from_millis(16)),
            Burst::None => {}
        }
        changed
    }

    pub fn clear_candidates(&mut self) {
        self.candidates.clear();
    }

    pub fn arm_for_scene(&mut self) {
        self.clean_phase = CleanPhase::Armed;
    }

    pub fn set_burst_for_scene(&mut self) {
        self.burst = Burst::At(0.0);
    }

    pub fn seed_undo_for_scene(&mut self) {
        self.quarantine.push(Quarantined {
            target: scan::CleanTarget {
                path: PathBuf::from("/home/demo/.cache/old-session"),
                label: "Old session stash".into(),
                detail: "Ready to restore".into(),
                kind: scan::Kind::Cache,
                impact: scan::Impact::Safe,
                bytes: 1024 * 1024,
                tool: None,
                default_on: false,
            },
            stash: PathBuf::from("/home/demo/.local/share/cleanx/quarantine/.cache/old-session"),
        });
    }

    pub fn set_low_volume(&mut self) {
        self.volumes = vec![scan::Volume {
            name: "System".into(),
            mount: "/".into(),
            total: 500_000_000_000,
            used: 465_000_000_000,
            free: 35_000_000_000,
        }];
        self.volume = 0;
    }
}

pub fn app_poll(s: &mut super::State, t: f64, tx: &mut Tx) -> bool {
    s.session.tick(t, tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_home() -> (PathBuf, PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("cleanx-s-{}-{nanos}", std::process::id()));
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        (base, home)
    }

    #[test]
    fn clean_stashes_and_undo_restores() {
        let (base, home) = temp_home();
        let dir = home.join(".cache").join("demo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), b"keep").unwrap();
        let target = scan::CleanTarget {
            path: dir.clone(),
            label: "Demo".into(),
            detail: "test".into(),
            kind: scan::Kind::Cache,
            impact: scan::Impact::Safe,
            bytes: 4,
            tool: None,
            default_on: true,
        };
        let mut s = Session::new();
        s.set_candidates(vec![target]);
        assert!(s.arm());
        let report = s.clean_with_home(&home);
        assert_eq!(report.path_items, 1);
        assert_eq!(s.held_bytes(), 4);
        assert_eq!(s.freed_bytes(), 0);
        assert!(!dir.exists());
        assert_eq!(s.quarantine().len(), 1);

        let undo = s.undo_with_home(&home);
        assert_eq!(undo.restored, 1);
        assert_eq!(s.held_bytes(), 0);
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "keep");
        assert_eq!(s.candidates().len(), 1);
        assert!(!s.candidates()[0].chosen);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn undo_disarms_the_clean_action() {
        let (base, home) = temp_home();
        let dir = home.join(".cache").join("one");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), b"x").unwrap();
        let mut s = Session::new();
        s.set_candidates(vec![scan::CleanTarget {
            path: dir,
            label: "One".into(),
            detail: "test".into(),
            kind: scan::Kind::Cache,
            impact: scan::Impact::Safe,
            bytes: 1,
            tool: None,
            default_on: true,
        }]);
        s.arm();
        s.clean_with_home(&home);
        s.set_candidates(vec![scan::CleanTarget {
            path: home.join(".cache").join("two"),
            label: "Two".into(),
            detail: "test".into(),
            kind: scan::Kind::Cache,
            impact: scan::Impact::Safe,
            bytes: 1,
            tool: None,
            default_on: true,
        }]);
        assert!(s.arm());
        s.undo_with_home(&home);
        assert!(!s.armed());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn a_selected_child_does_not_stash_twice() {
        let (base, home) = temp_home();
        let child = home.join(".cache").join("thumbnails");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("t.png"), b"t").unwrap();
        fs::write(home.join(".cache").join("big"), b"b").unwrap();
        let target = |path: PathBuf, bytes: u64| scan::CleanTarget {
            path,
            label: "Cache".into(),
            detail: "test".into(),
            kind: scan::Kind::Cache,
            impact: scan::Impact::Safe,
            bytes,
            tool: None,
            default_on: true,
        };
        let mut s = Session::new();
        s.set_candidates(vec![target(child, 1), target(home.join(".cache"), 2)]);
        s.arm();
        let report = s.clean_with_home(&home);
        assert_eq!(report.path_items, 1);
        assert_eq!(report.failed, 0);
        assert_eq!(s.held_bytes(), 2);
        assert!(!home.join(".cache").exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn tool_targets_never_offer_undo() {
        let target = scan::CleanTarget {
            path: PathBuf::from("/var/cache/dnf"),
            label: "dnf".into(),
            detail: "test".into(),
            kind: scan::Kind::Cache,
            impact: scan::Impact::Safe,
            bytes: 8,
            tool: Some(scan::Tool {
                program: "dnf",
                args: &["clean", "packages"],
                privileged: true,
            }),
            default_on: true,
        };
        let mut s = Session::new();
        s.set_candidates(vec![target]);
        assert!(!s.candidates()[0].undoable);
    }

    #[test]
    fn a_view_only_item_cannot_be_selected_or_cleaned() {
        let (base, home) = temp_home();
        let dir = home.join(".config");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("settings"), b"x").unwrap();
        let mut s = Session::new();
        s.set_candidates(vec![scan::CleanTarget {
            path: dir.clone(),
            label: "Config".into(),
            detail: "test".into(),
            kind: scan::Kind::Meta,
            impact: scan::Impact::HandsOff,
            bytes: 1,
            tool: None,
            default_on: true,
        }]);
        assert!(!s.candidates()[0].chosen);
        s.toggle(0);
        assert!(!s.candidates()[0].chosen);
        assert!(!s.arm());
        let report = s.clean_with_home(&home);
        assert_eq!(report.path_items, 0);
        assert!(dir.exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn held_items_survive_a_new_session() {
        let (base, home) = temp_home();
        let dir = home.join(".cache").join("keep");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), b"x").unwrap();
        let mut s = Session::new();
        s.set_candidates(vec![scan::CleanTarget {
            path: dir.clone(),
            label: "Keep".into(),
            detail: "test".into(),
            kind: scan::Kind::Cache,
            impact: scan::Impact::Safe,
            bytes: 1,
            tool: None,
            default_on: true,
        }]);
        s.arm();
        s.clean_with_home(&home);

        let mut reopened = Session::new();
        reopened.load_quarantine_with_home(&home);
        assert_eq!(reopened.quarantine().len(), 1);
        assert_eq!(reopened.held_bytes(), 1);
        let undo = reopened.undo_with_home(&home);
        assert_eq!(undo.restored, 1);
        assert!(dir.exists());
        assert_eq!(reopened.held_bytes(), 0);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn scan_speeds_up_the_spin_clock() {
        let mut s = Session::new();
        let mut tx = Tx::default();
        let start = s.spin_phase();
        for i in 0..30 {
            s.tick(i as f64 / 60.0, &mut tx);
        }
        let idle = (s.spin_phase() - start).abs();

        s.scan_now();
        let scan_start = s.spin_phase();
        for i in 30..60 {
            s.tick(i as f64 / 60.0, &mut tx);
        }
        let scan = (s.spin_phase() - scan_start).abs();
        assert!(idle < 1e-6);
        assert!(scan > 0.1);
    }

    #[test]
    fn dragging_the_ring_moves_it_and_leaves_momentum() {
        let mut s = Session::new();
        let start = s.spin_phase();
        s.spin_drag_start(0.0);
        s.spin_drag_move(0.5);
        s.spin_drag_end();
        let dragged = s.spin_phase();
        assert!((dragged - start - 0.5).abs() < 1e-6);

        let mut tx = Tx::default();
        s.tick(1.0 / 60.0, &mut tx);
        assert!(s.spin_phase() > dragged);
    }
}
