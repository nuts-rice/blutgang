{ pkgs ? import <nixpkgs> {
  overlays = [
    (import (builtins.fetchTarball {
      url = "https://github.com/oxalica/rust-overlay/archive/master.tar.gz";
    }))
  ];
}}:

pkgs.mkShell {
  buildInputs = [
    (pkgs.rust-bin.stable.latest.default.override { 
      extensions = [ "rust-src" "rustfmt-preview" "rust-analyzer"];
    })
    pkgs.gcc
    pkgs.pkg-config
    pkgs.openssl
    pkgs.systemdLibs
  ];

  shellHook = ''
    export CARGO_BUILD_RUSTC_WRAPPER=$(which sccache)
    export RUSTC_WRAPPER=$(which sccache)
  '';
}
