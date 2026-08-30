/// Stamps the build's commit into the binary, so a pasted bug report names an exact commit.
/// The tag only reaches the binary as `CARGO_PKG_VERSION`, which every build between two tags
/// shares. `unknown` when git isn't there (a source tarball, a sandboxed builder).
fn stamp_git_sha() {
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|sha| sha.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=IWGUI_GIT_SHA={sha}");
    // Without this the stamp is baked once and never refreshed, so every later build would
    // report the commit that happened to be checked out the first time. Guarded on existence:
    // cargo treats a missing `rerun-if-changed` path as always-dirty, and in a git *worktree*
    // `.git` is a file, not a directory — declaring the path unconditionally would rerun this
    // script (and relink the binary) on every single build there.
    // ponytail: HEAD only. A commit made without switching refs still moves refs/heads/<branch>,
    // so the stamp can lag by one commit; follow the ref file too if that ever matters.
    if std::path::Path::new("../../.git/HEAD").is_file() {
        println!("cargo:rerun-if-changed=../../.git/HEAD");
    }
}

fn main() {
    stamp_git_sha();

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
