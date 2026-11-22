## Beta. Bugs expected. Testers wanted.

These are beta builds. Windows and macOS binaries are built in CI but have not been tested on real hardware. Linux is the only verified build. Expect bugs.

We need testers. Download a binary, use it, and tell us what breaks by [opening an issue](https://github.com/ChindanaiNaKub/cleanx/issues). Include:

- What you expected and what happened.
- Your OS and how you installed the app.
- The output of `bash scripts/verify.sh`, if you can run it.
- A scene command such as `cargo run -- --snap out.png --scene clean` when the bug is visual.

Back up files you cannot afford to lose before you run a beta build. Cleaning needs two presses and moves files to Quarantine, but a bug can still cause damage.
