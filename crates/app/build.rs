fn main() {
    // On Windows, embed the application icon and version metadata into the .exe.
    // The icon is derived from assets/icons/app-icon.png via scripts/gen-icons.ps1;
    // replace that PNG (and regenerate the .ico) to change the app icon.
    #[cfg(windows)]
    {
        let icon = "../../assets/icons/app-icon.ico";
        println!("cargo:rerun-if-changed={icon}");
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon);
        res.set("ProductName", "Islandora Workbench");
        res.set("FileDescription", "Islandora Workbench GUI");
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        if let Err(e) = res.compile() {
            // Don't hard-fail local non-release builds if the Windows SDK rc tooling
            // is unavailable; the icon is cosmetic. CI runners have the toolchain.
            println!("cargo:warning=failed to embed Windows resources: {e}");
        }
    }
}
