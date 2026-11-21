# Bounded context: disk hygiene

cleanX has one domain context. It makes disk use visible and moves removable items aside so the user can review them. Everything inside this boundary uses the words in [GLOSSARY.md](../GLOSSARY.md).

## In scope

- Disk volumes and free space, read with `df`.
- Candidates, the allow-listed items cleanX may act on.
- Read-only discovery of hidden and large items across home. These items carry an Impact tag; only allow-listed Candidates may be cleaned.
- Safe clean, the two-press action that stashes path items and runs owning tools.
- Session quarantine, undo, and the local change log.
- The freed-space counter and the scan visual.

## Outside the boundary

- Continuous monitoring, tray icons, and notifications.
- Malware removal, registry repair, and driver updates.
- Accounts, network calls, cloud storage, and telemetry.
- Editing system files. cleanX never deletes outside the home allow-list.
- Undo across restarts. Undo ends when the session ends.

## Language

Use the terms in `GLOSSARY.md`. Do not call a candidate junk or a safe clean a purge. The words carry rules about what the app may delete.

## Ownership

The `Session` type owns candidates, selection, arms, quarantine, history, and animation clocks. `scan.rs` owns every path and tool guard. The view reads session state, and press handlers call session methods. Only `scan.rs` performs the file moves for a clean.

## Adjacent contexts

Package tools such as dnf and Flatpak sit outside the boundary. cleanX calls their own clean commands and does not model their internals. Windows and macOS adapters will be separate contexts with their own rules.
