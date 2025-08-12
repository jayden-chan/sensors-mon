{
  description = "TUI program for monitoring sensor values from lm-sensors and NVML";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        craneLib = crane.mkLib pkgs;

        txtFilter = path: _type: builtins.match ".*txt$" path != null;
        txtOrCargo = path: type: (txtFilter path type) || (craneLib.filterCargoSources path type);

        gccSemver = builtins.elemAt (builtins.match "^([[:digit:]]+\\.[[:digit:]]+\\.[[:digit:]]+)\\.?.*$" (pkgs.lib.getVersion pkgs.stdenv.cc.cc)) 0;

        bindgenShellHook = ''
          export BINDGEN_EXTRA_CLANG_ARGS="$(< ${pkgs.stdenv.cc}/nix-support/libc-crt1-cflags) \
            $(< ${pkgs.stdenv.cc}/nix-support/libc-cflags) \
            $(< ${pkgs.stdenv.cc}/nix-support/cc-cflags) \
            $(< ${pkgs.stdenv.cc}/nix-support/libcxx-cxxflags) \
            ${pkgs.lib.optionalString pkgs.stdenv.cc.isClang "-idirafter ${pkgs.stdenv.cc.cc}/lib/clang/${pkgs.lib.getVersion pkgs.stdenv.cc.cc}/include"} \
            ${pkgs.lib.optionalString pkgs.stdenv.cc.isGNU "-isystem ${pkgs.stdenv.cc.cc}/include/c++/${pkgs.lib.getVersion pkgs.stdenv.cc.cc} -isystem ${pkgs.stdenv.cc.cc}/include/c++/${pkgs.lib.getVersion pkgs.stdenv.cc.cc}/${pkgs.stdenv.hostPlatform.config} -idirafter ${pkgs.stdenv.cc.cc}/lib/gcc/${pkgs.stdenv.hostPlatform.config}/${gccSemver}/include"}"
        '';

        commonArgs = {
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = txtOrCargo;
            name = "source";
          };

          env = {
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

            LMSENSORS_STATIC = "1";
            LMSENSORS_LIB_DIR = "${pkgs.lm_sensors.out}/lib";
            LMSENSORS_INCLUDE_DIR = "${pkgs.lm_sensors.dev}/include/sensors";
          };

          preConfigurePhases = [
            "bindgen"
          ];

          bindgen = bindgenShellHook;

          strictDeps = true;
          buildInputs = [
            pkgs.llvmPackages.libclang
            pkgs.lm_sensors
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv ];
        };

        sensors-mon = craneLib.buildPackage (
          commonArgs // { cargoArtifacts = craneLib.buildDepsOnly commonArgs; }
        );
      in
      {
        checks = {
          inherit sensors-mon;
        };

        packages.default = sensors-mon;

        apps.default = flake-utils.lib.mkApp { drv = sensors-mon; };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = [
            pkgs.lm_sensors
            pkgs.rust-analyzer
            pkgs.rustfmt
          ];

          env = {
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

            LMSENSORS_STATIC = "1";
            LMSENSORS_LIB_DIR = "${pkgs.lm_sensors.out}/lib";
            LMSENSORS_INCLUDE_DIR = "${pkgs.lm_sensors.dev}/include/sensors";
          };

          shellHook = bindgenShellHook;
        };
      }
    );
}
