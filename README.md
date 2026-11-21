<img src="assets/icon.svg" width="64" height="64" alt="">

# cleanX

A small [Rune](https://github.com/anomalyco/rune) app that shows free disk space and lists files you can safely remove.

## Overview

These are real snapshots from the app's headless renderer.

| | |
| --- | --- |
| **Free space dashboard**<br>The lava orb warms as the disk fills. Pills switch disks. Scan grows a sunburst.<br>![Space tab](assets/space.png) | **Safe-to-clean list**<br>Caches, temp files and junk with sizes and safety notes. Selected rows carry a lava tick.<br>![Clean tab](assets/clean.png) |
| **Two-press safety**<br>The first press arms the action. The second removes the checked rows.<br>![Confirm removal](assets/confirm.png) | **Undo**<br>Path items move to quarantine and restore until you close cleanX.<br>![Undo a clean](assets/undo.png) |

Regenerate them with `cargo run -- --snap assets/space.png` and, for the others, add `--scene clean`, `--scene confirm` or `--scene undo`.

## What it does

The Space tab draws a lava orb that shows free space and warms as the disk fills. Press Scan now to grow a sunburst. Pills switch between disks.

The Clean tab lists known caches, temp folders, trash, `.DS_Store` files and `node_modules` folders. On Fedora it also lists the dnf package cache, journal logs and unused Flatpak runtimes. Select rows, arm the action, then press again to clean. Path items move to quarantine and get an Undo button. System tools do not.

## Run it

One command gets the pinned Rune tree, builds, and installs `cleanx` into `~/.local/bin`. Add that folder to your `PATH`. The build needs nightly Rust.

```bash
git clone https://github.com/ChindanaiNaKub/cleanx.git && cd cleanx && bash scripts/install.sh
```

From a checkout next to `../rune-src`, `cargo run --` starts the app without installing.

Scenes render headlessly with `cargo run -- --snap out.png --scene clean`. Scenes are `clean`, `confirm`, `burst`, `burst-control`, `undo`, `empty`, `low` and `isolated`. Run `cargo test` for unit tests. Run `scripts/verify.sh` to check formatting, tests and every scene.

## Platform support and roadmap

- Fedora, today. The full clean list and the dnf, journalctl and Flatpak tools work here.
- Linux, today. The dashboard and the cache, temp and trash list work on any Unix-like system with `df`.
- macOS, next. The allow-list already covers Caches, Logs and Trash. Rendering needs the Metal backend and local testing.
- Windows, next. The allow-list already covers the temp folder and reads `USERPROFILE`. Rendering needs the DirectX 12 backend and local testing.

## How it stays safe

- Cleaning takes two presses. The first arms the action. The second acts on the checked rows.
- Path items move to `~/.local/share/cleanx/quarantine`, not to the trash. Undo restores them until you close cleanX. The app deletes that folder at the next start. Closed sessions cannot be undone.
- Only paths under your home directory move. Symlinks and paths whose real location is outside home are refused.
- System targets run their owning tool: `dnf clean packages`, `journalctl --vacuum-size=200M` and `flatpak uninstall --unused`. Privileged tools go through `pkexec`. If that fails, the notice shows a `sudo` command to copy into a terminal. No shell is used.
- The list is a fixed allow-list. `node_modules` is marked REBUILD and unchecked by default. App caches, journal logs, Trash and build folders are unchecked by default.
- Every successful clean appends a line to `~/.local/share/cleanx/history.log`.

## Limitations

- Disk totals come from `df -kP`. The Space tab needs a Unix-like system. The Clean tab uses `HOME` or `USERPROFILE` and works where those are set.
- Scanning happens at startup and on demand. The app does not watch the disk live.
- System targets appear only when `dnf`, `journalctl` or `flatpak` is on `PATH`. Privileged targets need a polkit agent for `pkexec`. The Flatpak size is an estimate from local Flatpak data.
- The scanner follows no symlinks. Discovery stops at 30,000 entries and size counting at 40,000 files. Reported sizes may be samples for very large trees.
- Undo ends when the session ends. The quarantine is cleared at startup, so a clean from a closed session cannot be restored.
- The app needs a GPU with a supported Vulkan driver on Linux. Headless snapshots use the same renderer.
- No build command or test removes a file. Only a confirmed clean moves path items to quarantine.
