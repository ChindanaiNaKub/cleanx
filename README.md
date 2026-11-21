<img src="assets/icon.svg" width="64" height="64" alt="">

# cleanX

A small [Rune](https://github.com/anomalyco/rune) app that makes disk space visible and reports files you can safely remove.

## What it shows

- **Space tab.** A molten lava orb shows the free-space percentage and glows warmer as the disk fills. Click the orb to open Clean. A generated sunburst grows beside it when you press Scan now. A split bar shows used against free, with three stat cards, a mesh-gradient reclaimable-space banner, and a running freed-space counter. Pills switch between disks.
- **Clean tab.** A reviewable list of known caches, temp folders, trash, `.DS_Store` files and `node_modules` folders. On Fedora it also lists the dnf package cache, journal logs and unused Flatpak runtimes. Selected rows hold drifting lava. Arming cleanup wraps the summary in a fiery rim. A short sparkle burst plays after a clean. Path items get an Undo button. System tools never do. Each row shows its size and kind. App caches, journal logs, Trash and build folders are unchecked by default. Recent log entries appear under the list.

## Run it

Rune is pre-release and needs its own patched dependencies. The app expects a shallow clone of Rune at `../rune-src`, then patches those vendored crates in `Cargo.toml`. Use a pinned revision. A moving `dev` branch can break the build. The toolchain file asks for nightly Rust.

```bash
git clone --depth 1 --branch dev https://github.com/anomalyco/rune.git ../rune-src
git -C ../rune-src checkout b39e1b4d326e323e971b86b8f105d3df53d4642c
cargo run --
```

Agent tooling works in debug builds.

```bash
cargo run -- --snap default.png
cargo run -- --snap clean.png --scene clean
cargo run -- --snap low.png --scene low
cargo run -- --snap confirm.png --scene confirm
cargo run -- --snap burst.png --scene burst --at 0.35
cargo run -- --snap burst-control.png --scene burst-control --at 0.35
cargo run -- --snap undo.png --scene undo
cargo run -- --snap isolated.png --scene isolated
cargo test
```

Scenes are `clean`, `confirm`, `burst`, `burst-control`, `undo`, `empty`, `low` and `isolated`. The `isolated` scene and `tests/fx_regression.rs` guard the effect layers against a renderer that forgets an effect target format. Snapshot tooling skips the startup quarantine purge, so taking a screenshot never deletes a stash. Run `scripts/verify.sh` to check formatting, tests, all scenes, and that no two scenes render the same pixels.

## Safety

- Cleaning needs two presses. The first arms the action, the second acts on the checked rows.
- Path items move to `~/.local/share/cleanx/quarantine` instead of being deleted. Undo restores them until you close cleanX. At the next start, cleanX deletes that folder. Closed sessions cannot be undone.
- Only paths under your home directory are moved. Symlinks and paths whose real location is outside home are refused.
- System targets run their owning tool: `dnf clean packages`, `journalctl --vacuum-size=200M` and `flatpak uninstall --unused`. Privileged tools go through `pkexec`. If that fails, the notice shows a `sudo` command to copy into a terminal. No shell is used.
- Only a fixed allow-list of cache, temp, trash and OS-junk paths is listed. `node_modules` is marked `REBUILD` and unchecked by default.
- The scanner follows no symlinks and caps its walk at 30,000 entries per discovery and 40,000 files per size calculation. Reported sizes can be samples for very large trees.
- Every successful clean appends a line to `~/.local/share/cleanx/history.log`.

## Limits

- Disk totals come from `df -kP`, so the Space tab needs a Unix-like system. The Clean tab uses `HOME` or `USERPROFILE` and works where those are set.
- The app scans on startup. Press Scan now to refresh. It does not watch the disk live.
- System targets appear only when `dnf`, `journalctl` or `flatpak` is on `PATH`. Privileged targets need a polkit agent for `pkexec`. The Flatpak size is an estimate from local Flatpak data, not the exact unused-runtime total.
- No file is removed by this repository's tests or by any command run while building it.
