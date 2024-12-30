{
  description = "blazingly fast tool for peeking at codebases";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "glimpse";
          version = "0.6.2";
          
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [ ];

          checkFlags = [
            "--skip=tokenizer::tests::test_hf_counter"
          ];

          meta = with pkgs.lib; {
            description = "A blazingly fast tool for peeking at codebases";
            homepage = "https://github.com/seatedro/glimpse";
            license = licenses.mit;
            maintainers = ["seatedro"];
            platforms = platforms.all;
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-bin.stable.latest.default
            pkg-config
          ];
        };
      }
    );
}
