{
  description = "Linux development environment for Islandora Workbench GUI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forEachSystem = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forEachSystem (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rustfmt" "clippy" ];
          };
          runtimeLibraries = with pkgs; [
            libxkbcommon
            wayland
            vulkan-loader
            libGL
            libx11
            libxcb
            libdrm
            libgbm
            libxcomposite
            libxdamage
            libxext
            libxfixes
            libxrandr
          ];
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustToolchain
              clang
              cmake
              git
              pkg-config
              python3
              uv
            ];

            buildInputs = with pkgs; [
              alsa-lib
              fontconfig
              freetype
              glib
              openssl
              sqlite
              zlib
              zstd
            ] ++ runtimeLibraries;

            # GPUI loads the Wayland and graphics drivers dynamically. NixOS does not
            # expose those libraries through a global /usr/lib, so retain their store paths.
            LD_LIBRARY_PATH = nixpkgs.lib.makeLibraryPath runtimeLibraries;
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          };
        });
    };
}
