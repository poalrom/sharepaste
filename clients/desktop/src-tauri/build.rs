fn main() {
    // `tauri_build::build()` only declares the config files and `capabilities/`
    // as build inputs. The icons are read much later, by the
    // `tauri::generate_context!` proc macro — and cargo directives printed from
    // a proc macro go nowhere, so nothing here tracks them. Without these lines
    // an icon edit leaves the build script cached, the crate unrecompiled, and
    // the previous icon embedded in the binary (`cargo build` says "Finished"
    // in half a second and `tauri dev` keeps showing the old artwork).
    for icon in [
        "icons/icon.png",
        "icons/icon@2x.png",
        "icons/128x128@2x.png",
        "icons/32x32@2x.png",
        "icons/icon.ico",
        "icons/tray-template.png",
        "icons/tray.png",
    ] {
        println!("cargo:rerun-if-changed={icon}");
    }

    tauri_build::build();
}
