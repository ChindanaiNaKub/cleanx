# Contributing to cleanX

Thanks for helping. cleanX removes files from a user's disk, so a small bug can cost someone data. These rules keep the app safe as it grows.

By contributing, you agree that your work is licensed under the [MIT License](LICENSE).

## Set up

Follow **Run it** in the [README](README.md). In short:

- Install nightly Rust with `rustfmt`.
- Put the pinned Rune tree next to this checkout at `../rune-src`. `bash scripts/rune.sh` fetches it.
- Build and run with `cargo run --`.

`bash scripts/install.sh` builds and installs `cleanx` into `~/.local/bin` if you want the installed app.

## Before you open a pull request

Run the same gate that CI runs:

```bash
bash scripts/verify.sh
```

The script checks formatting, runs the tests, renders every scene, and checks that no two scenes are identical. It exits non-zero on any failure. Do not open a pull request with a failing gate.

The three parts, if you want to run them by hand:

```bash
cargo fmt -- --check
cargo test
cargo run -- --snap out.png --scene clean
```

CI runs `cargo fmt -- --check` and `cargo test` on Linux, Windows and macOS. It runs the full `scripts/verify.sh` on Linux. Match that coverage when you change rendering or platform code.

## Write code

- Let `rustfmt` format all Rust. Do not hand-align code.
- Use the words in [GLOSSARY.md](GLOSSARY.md). Say **Scan**, not sweep. Say **Impact tag**, not risk badge. If you need a new word, add it to the glossary with an `_Avoid_` line.
- Keep one change per pull request. A fix and a refactor belong in separate pull requests.
- Keep the subject line short and in the imperative, for example `Add dnf cache size estimate`.

## Keep the safety rules

These rules are the core of the app. Do not weaken them. A change to `src/scan.rs` or `src/session.rs` must keep all of them true:

- A clean needs two presses. The first arms it. The second removes the checked rows.
- Only paths under the user's home directory move. Symlinks and paths whose real location is outside home are refused.
- A **Protected folder** is reported but never offered. It cannot be selected.
- Path items move to **Quarantine**, never to the trash. Undo restores them until the user clears held items. Clearing held items cannot be undone.
- System targets run their **Owning tool** such as `dnf clean packages`. No shell is used.
- A **Scan** never removes a file. No build command or test removes a file.

Add or update a test whenever you change discovery, selection, quarantine or removal. A safety rule without a test is not enforced.

## Change the design

- Add a new domain word to [GLOSSARY.md](GLOSSARY.md).
- Write an ADR in [docs/adr/](docs/adr/) when you make a design choice that future contributors will ask about. Use the existing numbered format. Mark an old ADR as superseded with a `status:` line in its front matter instead of deleting it.

## Report a bug

Open an issue with:

- What you expected and what happened.
- Your OS and how you built or installed the app.
- The output of `bash scripts/verify.sh`, if you can run it.
- A headless scene command, such as `cargo run -- --snap out.png --scene clean`, when the bug is visual.
