//! The headless case that used to panic when an effect slot sat in an isolated layer.

use std::process::Command;

#[test]
fn effects_render_inside_an_isolated_layer() {
    let shot = std::env::temp_dir().join("cleanx-isolated-fx.png");
    let run = Command::new(env!("CARGO_BIN_EXE_cleanx"))
        .args(["--scene", "isolated", "--snap"])
        .arg(&shot)
        .output()
        .expect("run cleanx headlessly");
    assert!(
        run.status.success(),
        "isolated effect layer failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(shot.exists(), "no snapshot was written");
}
