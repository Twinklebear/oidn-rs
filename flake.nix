{
  description = "Rust bindings for Intel Open Image Denoise";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: let
    lib = nixpkgs.lib;
    systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin"];
    forAllSystems = f: lib.genAttrs systems (system: f system nixpkgs.legacyPackages.${system});
    # The crate version is the Open Image Denoise version it wraps.
    oidnVersion = (lib.importTOML ./Cargo.toml).package.version;
  in {
    formatter = forAllSystems (_: pkgs: pkgs.alejandra);

    packages = forAllSystems (_: pkgs: {
      openimagedenoise = pkgs.openimagedenoise.overrideAttrs (_: {
        version = oidnVersion;
        src = pkgs.fetchzip {
          url = "https://github.com/RenderKit/oidn/releases/download/v${oidnVersion}/oidn-${oidnVersion}.src.tar.gz";
          hash = "sha256-9+ednBoFvkC1AHZxsGrwyfQTQqglV+F5bYR8VJX3K8w=";
        };
      });
    });

    devShells = forAllSystems (system: pkgs: let
      oidn = self.packages.${system}.openimagedenoise;
    in {
      default = pkgs.mkShell {
        packages = with pkgs; [cargo clippy rustc rustfmt rust-analyzer oidn];
        # Its CMake build installs no pkg-config file, so point build.rs at it.
        OIDN_DIR = "${oidn}";
        # bindgen, run by `cargo xtask`, loads libclang.
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        # Tests and examples load the library at runtime, and the interop tests open Vulkan.
        LD_LIBRARY_PATH = lib.optionalString pkgs.stdenv.hostPlatform.isLinux (lib.makeLibraryPath [oidn pkgs.vulkan-loader]);
        DYLD_FALLBACK_LIBRARY_PATH = lib.optionalString pkgs.stdenv.hostPlatform.isDarwin "${oidn}/lib";
      };
    });
  };
}
