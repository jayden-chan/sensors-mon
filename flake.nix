{
  description = "TUI program for monitoring sensor values from lm-sensors and NVML";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    devenv.url = "github:cachix/devenv";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
  };

  nixConfig = {
    extra-trusted-public-keys = "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw=";
    extra-substituters = "https://devenv.cachix.org";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      devenv,
      ...
    }@inputs:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        craneLib = crane.mkLib pkgs;

        txtFilter = path: _type: builtins.match ".*txt$" path != null;
        txtOrCargo = path: type: (txtFilter path type) || (craneLib.filterCargoSources path type);

        commonArgs = {
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = txtOrCargo;
            name = "source";
          };

          strictDeps = true;
          buildInputs = [ ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv ];
        };

        sensors-mon = craneLib.buildPackage (
          commonArgs // { cargoArtifacts = craneLib.buildDepsOnly commonArgs; }
        );
      in
      {
        checks = {
          inherit sensors-mon;
        };

        packages.devenv-up = self.devShells.${system}.default.config.procfileScript;
        packages.devenv-test = self.devShells.${system}.default.config.test;

        packages.default = sensors-mon;

        apps.default = flake-utils.lib.mkApp { drv = sensors-mon; };

        devShells.default = devenv.lib.mkShell {
          inherit inputs pkgs;
          modules = [
            (
              { pkgs, config, ... }:
              {
                packages = [ pkgs.lm_sensors ];

                env = {
                  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
                  LMSENSORS_LIB_DIR = "${pkgs.lm_sensors}/lib";
                };

                languages.rust.enable = true;
                languages.rust.channel = "stable";

                enterShell = ''
                  export BINDGEN_EXTRA_CLANG_ARGS="$(< ${pkgs.stdenv.cc}/nix-support/libc-crt1-cflags) \
                    $(< ${pkgs.stdenv.cc}/nix-support/libc-cflags) \
                    $(< ${pkgs.stdenv.cc}/nix-support/cc-cflags) \
                    $(< ${pkgs.stdenv.cc}/nix-support/libcxx-cxxflags) \
                    ${pkgs.lib.optionalString pkgs.stdenv.cc.isClang "-idirafter ${pkgs.stdenv.cc.cc}/lib/clang/${pkgs.lib.getVersion pkgs.stdenv.cc.cc}/include"} \
                    ${pkgs.lib.optionalString pkgs.stdenv.cc.isGNU "-isystem ${pkgs.stdenv.cc.cc}/include/c++/${pkgs.lib.getVersion pkgs.stdenv.cc.cc} -isystem ${pkgs.stdenv.cc.cc}/include/c++/${pkgs.lib.getVersion pkgs.stdenv.cc.cc}/${pkgs.stdenv.hostPlatform.config} -idirafter ${pkgs.stdenv.cc.cc}/lib/gcc/${pkgs.stdenv.hostPlatform.config}/${pkgs.lib.getVersion pkgs.stdenv.cc.cc}/include"}"
                '';
              }
            )
          ];
        };
      }
    );
}
