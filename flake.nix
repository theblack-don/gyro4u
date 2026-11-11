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
          ds4u = pkgs.rustPlatform.buildRustPackage {
            pname = "ds4u";
            version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            doCheck = false;

            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = with pkgs; [
              alsa-lib
              libxkbcommon
              openssl
              udev
            ];

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
