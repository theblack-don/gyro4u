{
  description = "DualSense 4 You";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          # winit and wgpu load these with dlopen at runtime, so they need to
          # be on the binary's RUNPATH rather than just in buildInputs.
          runtimeLibs = with pkgs; [
            libglvnd
            libx11
            libxcb
            libxcursor
            libxi
            libxkbcommon
            libxrandr
            libxrender
            vulkan-loader
            wayland
          ];

          ds4u = pkgs.rustPlatform.buildRustPackage {
            pname = "ds4u";
            version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            doCheck = false;

            nativeBuildInputs = [
              pkgs.patchelf
              pkgs.pkg-config
            ];
            buildInputs = [
              pkgs.alsa-lib
              pkgs.openssl
              pkgs.udev
            ] ++ runtimeLibs;

            postFixup = ''
              patchelf --add-rpath "${pkgs.lib.makeLibraryPath runtimeLibs}" "$out/bin/ds4u"
            '';

            meta = with pkgs.lib; {
              description = "DualSense 4 You";
              homepage = "https://git.yokai.digital/deadYokai/ds4u";
              license = licenses.mit;
              mainProgram = "ds4u";
              platforms = platforms.linux;
            };
          };
        in
        {
          inherit ds4u;
          default = ds4u;
        }
      );

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.ds4u}/bin/ds4u";
        };
      });
    };
}
