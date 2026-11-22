# cleanx feature and design research

Goal: decide which features a cross-platform disk cleaner needs, and which spinning visual direction fits cleanx. This document records research. It changes no runtime code.

## What cleanx already does

cleanX shows free space in an Orb, runs a read-only Scan, and lists reclaimable items with Impact tags. It cleans with two presses. The first press arms the action, and the second moves path items to Quarantine or runs an owning tool. Held items survive a restart. Protected folders such as `.config` and `.ssh` are reported but cannot be selected. Fedora gets dnf, journal, and Flatpak tools. Any Unix-like system gets the dashboard and the cache, temp, and trash list.

The gap is platform coverage and the breadth of the reclaimable list. The design already has the right primitives for a spinning treatment: the Orb, the sunburst, lava metaballs, glow, embers, and a burst state machine in `src/session.rs`.

## Feature shortlist

The shortlist uses the words in `GLOSSARY.md`. Each feature either extends the Scan and safe-clean loop or stays outside the boundary.

| Feature | Why it earns a place | Boundary fit |
| --- | --- | --- |
| Per-platform reclaimable rules | Each OS hides cache in different folders. A shared rule set cannot cover all three. | In. New platform adapters sit outside the core, as `bounded-context.md` already states. |
| Own cleanup tools for system caches | An owning tool knows what is safe. `dnf clean`, `journalctl --vacuum-size`, and DISM do work that hand deletion cannot. | In. Extend the owning-tool pattern. |
| All browser profiles, cache only | Chrome and Edge keep a cache per profile. Cleaning only Default frees a fraction of the space. | In. Keep cookies, history, and passwords out. |
| App leftover discovery | Dragging an app to Trash leaves support files behind. Match leftovers by bundle identifier on macOS and by install record on Windows. | In as review only. Show an Impact tag and never auto-select. |
| In-use file checks | A cleaner that deletes a file an app holds open can lose unsaved work. Ask the OS before deleting. On Windows use Restart Manager. | In. This is a guard at the delete boundary. |
| Duplicate and large-file review | A cache list misses large files and duplicates. BleachBit leaves them out, while WizTree and CleanMyMac include them. | In as Scan results with Impact tags. Never auto-select. |
| Age-based selection for temp files | Files older than a day or three days are almost always safe. Fresh files may still be in use. | In. Encode the age rule in the platform adapter. |
| Free-space report by category | Users need to see where space went before they trust the clean list. CleanMate and Diskhoji both lead with a map. | In. This is the existing sunburst and Orb. |
| Scheduled clean | CCleaner sells this. It also removes the user from the review step, which is the main safety feature. | Out for now. Revisit after the core loop is stable. |
| Secure shred and free-space wipe | BleachBit offers both. They are slow and irreversible, and they conflict with Quarantine and undo. | Out. cleanX keeps a held-then-cleared model. |
| Registry repair and driver tools | Microsoft does not support registry cleaners. They give little benefit and can break updates. | Out. `bounded-context.md` already excludes them. |
| Malware scan, network calls, telemetry | These need a second product and a trust model that cleanX does not have. | Out. Stay local and offline. |

## Platform matrix

The table names concrete targets for each platform. Linux is live today. macOS and Windows need their own adapters and rendering backends.

| Area | Linux, today | macOS, next | Windows, next |
| --- | --- | --- | --- |
| Temp and cache | `~/.cache`, `/tmp` entries owned by the user | `~/Library/Caches`, sandboxed caches under `~/Library/Containers/*/Data/Library/Caches` | `%LOCALAPPDATA%\Temp`, `C:\Windows\Temp`, with age filtering |
| Logs | journal via `journalctl` | `~/Library/Logs`, `DiagnosticReports` | CBS logs and WER archives older than 30 days |
| Trash | `~/.local/share/Trash` | `~/.Trash` | Recycle Bin, through the shell API |
| Package and system tools | `dnf clean packages`, `flatpak uninstall --unused`, apt and snap on other distros | No package cache tool needed. Report local Time Machine snapshots as managed by macOS. | `Delete-DeliveryOptimizationCache`, DISM `StartComponentCleanup`, Windows Update Cleanup |
| Browser cache | Chrome, Firefox, and Chromium cache folders | Same, across all profiles | Same, across all profiles, with browsers closed |
| Large and hidden files | Home scan, as today | Home scan, skip `/System`, `/usr`, and `/private/var` | Home scan, skip `WinSxS`, `Installer`, `pagefile.sys`, and `hiberfil.sys` |
| App leftovers | Review only | Match by bundle identifier in Application Support, Containers, Preferences, and Logs. Leave `com.apple` and Group Containers alone. | Match by uninstall registry key. Leave Program Files and user documents alone. |
| Protected folders | `.config`, `.ssh`, and paths whose real location leaves home | `com.apple` folders, Group Containers, `/Library/LaunchDaemons` | `C:\Windows\WinSxS`, `C:\Windows\Installer`, system profile folders |

## Spinning design direction

The user wants a spinning, or "ปั่นๆ", treatment. Keep the Orb. Add rotation to the primitives that already exist. Do not rebuild the renderer.

### Direction A: momentum spin, recommended

The Orb becomes a slow rotating disc. A ring of category arcs orbits the center. Each arc keeps the Impact tag color: SAFE is lava, CAUTION is amber, and HANDS OFF is a dim locked segment. Protected folders get no arc.

Motion follows the scan state. At rest the disc drifts. On Scan the disc accelerates, and the sunburst grows from the spinning center, so growth reads as motion. A user can drag the Orb to spin it. Release adds momentum, and friction brings it to rest. The gesture can start a Scan on release.

Cleaning reuses the burst state machine. Selected arcs detach, spiral inward, and disappear into the core. The freed-space counter rises as they land. Undo sends the arcs back out. A tight inner ring shows held space, which persists across restarts.

This direction wins on two points. It maps to the existing sunburst, burst state, lava effect, and ember effect. The spin also shows scan progress and clean progress instead of only decorating the screen.

### Direction B: orbit rings

Each reclaimable category gets its own concentric ring. Rings orbit at different speeds, with safe categories on the outside. Selecting a category pulls its ring forward and dims the others. Cleaning drops the selected ring into the core.

This direction reads as a data display. It gives up the playful feel. It costs more layout work and more careful color use, because rings share the same center. Pick this direction if precision matters more than play.

### Rules for either direction

- Spin speed carries state, so a stopped disc means idle or reduced motion.
- The spin never blocks input. A press can stop it at any point.
- Under reduced motion, show final arc positions and skip spin, count-up, and spiral effects.
- Keep a text readout of used space and freed space beside the disc. The ring is never the only source of a number.
- Prototype the motion in throwaway HTML before changing the Rust renderer.

## Working from Linux alone

You can do most of this work on Linux. You cannot finish it on Linux.

Measured in this checkout. Rust has two targets installed, `wasm32-unknown-unknown` and `x86_64-unknown-linux-gnu`. There is no mingw-w64 linker and no `.github` workflow. The host is `x86_64-unknown-linux-gnu` and the pinned toolchain is nightly 1.102.0. `../rune-src` is present, so local builds work today.

Split the work by what each machine can prove.

- Shared rules. Write platform adapters behind one candidate shape and test them on Linux with fixture paths. `tests/fx_regression.rs` and `scripts/verify.sh` already give you that harness.
- Compile checks. Add the `x86_64-pc-windows-gnu` target and mingw-w64 to type-check Windows code. This is inferred, not tested here. The patched `winit`, `wgpu-hal`, and `accesskit_windows` crates may fail for reasons outside cleanx.
- Real builds. Use CI runners. GitHub documents that standard runners are free and unlimited on public repositories. The labels `windows-latest`, `macos-latest`, and `macos-15-intel` cover both architectures. A private repository gets $12 of monthly compute on GitHub Free, and macOS costs $0.062 per minute, so a public repository is the cheap path.
- Rendering. The GPU backends need a real OS. Metal needs macOS and DirectX 12 needs Windows. Run the headless scenes in CI and keep the PNG byte check from `scripts/verify.sh` as the gate.

You do not need to buy a Mac. You need a public repository and a CI workflow.

## Build order

1. Add a CI workflow that runs `scripts/verify.sh` on Linux, Windows, and macOS. This gives you real proof before you widen the code.
2. Add macOS and Windows adapters for temp, cache, logs, trash, and browser cache. Each adapter returns the same candidate shape that `scan.rs` uses today. Test the rules with fixtures on Linux, then confirm them in CI.
3. Add in-use file checks and age rules at the delete boundary, before widening the candidate list.
4. Add all-profile browser caches and app leftover review, both unchecked by default.
5. Prototype Direction A as a throwaway page, then port the chosen motion to the Orb and sunburst.
6. Add duplicate and large-file review to the Scan results after the platform lists are stable.

Keep registry repair, secure shred, scheduling, and telemetry out of this sequence.
