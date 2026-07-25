// `generate_context!` reads two things this crate does not own:
//   * OUT_DIR, which Cargo only sets for crates with a build script;
//   * the resolved ACL manifests, which `tauri_build` generates from the app's
//     `capabilities/` and the core plugins' permission sets. Without them every
//     ACL lookup fails with "Plugin not found", which is indistinguishable from
//     the denial this crate exists to detect.
//
// `tauri_build` resolves everything relative to the current directory, so run it
// from the app crate. That makes the app's real config and capabilities the
// input while the generated output still lands in this crate's OUT_DIR.
fn main() {
    for p in [
        "../src-tauri/tauri.conf.json",
        "../src-tauri/capabilities",
        "../src-tauri/src/lib.rs",
    ] {
        println!("cargo:rerun-if-changed={p}");
    }

    let here = std::env::current_dir().expect("build script has a cwd");
    std::env::set_current_dir(here.join("../src-tauri")).expect("app crate is a sibling");
    tauri_build::build();
    std::env::set_current_dir(here).expect("restore cwd");
}
