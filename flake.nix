{
  description = "TUI program for monitoring sensor values from lm-sensors";

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

        commonArgs = {
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = txtOrCargo;
            name = "source";
          };

          preConfigurePhases = [ "env" ];
          env = ''export LMSENSORS_STATIC=1'';

          strictDeps = true;
          buildInputs = [ ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [ pkgs.libiconv ];
        };

        lm-sensors-mon = craneLib.buildPackage (
          commonArgs // { cargoArtifacts = craneLib.buildDepsOnly commonArgs; }
        );
      in
      {
        checks = {
          inherit lm-sensors-mon;
        };

        packages.default = lm-sensors-mon;

        apps.default = flake-utils.lib.mkApp { drv = lm-sensors-mon; };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = [ ];
        };
      }
    );
}
