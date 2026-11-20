{
  description = "Gyro4U";

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

          gyro4u = pkgs.rustPlatform.buildRustPackage {
            pname = "gyro4u";
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
              patchelf --add-rpath "${pkgs.lib.makeLibraryPath runtimeLibs}" "$out/bin/gyro4u"
            '';

            postInstall = ''
              install -Dm644 assets/icon.svg $out/share/icons/hicolor/scalable/apps/gyro4u.svg
              install -Dm644 assets/gyro4u.desktop $out/share/applications/gyro4u.desktop
            '';

            meta = with pkgs.lib; {
              description = "Gyro4U — DualSense controller utility";
              homepage = "https://github.com/theblack-don/gyro4u";
              license = licenses.mit;
              mainProgram = "gyro4u";
              platforms = platforms.linux;
            };
          };
        in
        {
          inherit gyro4u;
          default = gyro4u;
        }
      );

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.gyro4u}/bin/gyro4u";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            pkg-config
            alsa-lib
            openssl
            udev
            libxkbcommon
            libx11
            libxcursor
            libxi
            libxrandr
            libxrender
            libglvnd
            vulkan-loader
            wayland
            evtest
            libinput
          ];
        };
      });
    };
}
