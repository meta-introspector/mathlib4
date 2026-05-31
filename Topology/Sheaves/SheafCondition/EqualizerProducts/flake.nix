# Generated flake.nix for Topology.Sheaves.SheafCondition.EqualizerProducts
{
  description = "Mathlib module: Topology.Sheaves.SheafCondition.EqualizerProducts";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      packages.${system}.default = pkgs.stdenv.mkDerivation {
        pname = "mathlib-module-Topology.Sheaves.SheafCondition.EqualizerProducts";
        version = "0.1.0";
        src = ./.;
        nativeBuildInputs = [ pkgs.lean4 ];
        dontInstall = true;
        buildPhase = ''
          runHook preBuild
          lean --make $src 2>&1 || echo "Build note: may need dependencies"
          runHook postBuild
        '';
        installPhase = ''
          mkdir -p $out
          cp *.lean $out/ 2>/dev/null || true
          cp -r *.olean $out/ 2>/dev/null || true
        '';
      };
    };
}
