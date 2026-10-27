//! Embeds the app icon as a Windows resource so Explorer, the desktop shortcut and Alt-Tab
//! show it for `beatslopper.exe`. The window's own title-bar and taskbar icon is set at
//! runtime from `assets/icon.png` (see `native_options` in `src/main.rs`).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "BeatSlopper");
        res.set("ProductName", "BeatSlopper");
        res.compile()
            .expect("embedding assets/icon.ico (needs rc.exe from the Windows SDK)");
    }
}
