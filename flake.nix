{
  description = "blazingly fast tool for peeking at codebases";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "glimpse";
          version = "0.7.7";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            openssl
          ];

          buildInputs =
            with pkgs;
            [
              openssl
              cacert
            ]
            ++ lib.optionals stdenv.isDarwin [
              darwin.apple_sdk.frameworks.SystemConfiguration
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.CoreFoundation
              darwin.apple_sdk.frameworks.CoreServices
            ];

          preCheck = ''
            export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
          '';
          checkFlags = [
            "--skip=tokenizer::tests::test_hf_counter"
          ];

          meta = with pkgs.lib; {
            description = "A blazingly fast tool for peeking at codebases";
            homepage = "https://github.com/seatedro/glimpse";
            license = licenses.mit;
            maintainers = [ "seatedro" ];
            platforms = platforms.all;
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs =
            with pkgs;
            [
              rust-bin.stable.latest.default
              pkg-config
              openssl
              cacert
            ]
            ++ lib.optionals stdenv.isDarwin [
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.SystemConfiguration
              darwin.apple_sdk.frameworks.CoreFoundation
              darwin.apple_sdk.frameworks.CoreServices
            ];

          # Set OPENSSL_DIR for local development
          shellHook = ''
            export OPENSSL_DIR="${pkgs.openssl.dev}"
            export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
            export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
          '';
        };
      }
    );
}
