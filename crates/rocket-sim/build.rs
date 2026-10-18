use std::process::Command;
fn main() {
    for path in [
        "../../.git/HEAD",
        "../../.git/index",
        "../../crates",
        "../../Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "unknown".into(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .is_ok_and(|o| !o.stdout.is_empty());
    println!(
        "cargo:rustc-env=AETHER_SOURCE_REVISION={revision}{}",
        if dirty { "-dirty" } else { "" }
    );
}
