## cleanX v0.1.0

First stable release. Prebuilt binaries for Linux x86_64, Windows x86_64, macOS Intel and macOS Apple Silicon. No Rust or git needed.

The app runs on real hardware on Linux, macOS and Windows. macOS is tested on Apple Silicon with the Metal backend; an Intel binary is also available. Windows uses the DirectX 12 backend.

Downloaded binaries now include Rune's tooling, so the documented headless commands work from a downloaded binary:

```
cleanx --list-scenes
cleanx --snap out.png --scene clean
```

Cleaning needs two presses and moves files to Quarantine, which you can undo. Back up files you cannot afford to lose.

Found a bug? [Open an issue](https://github.com/ChindanaiNaKub/cleanx/issues) with your OS, how you installed the app, and what you expected.
