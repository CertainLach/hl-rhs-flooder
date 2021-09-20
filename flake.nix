{
  description = "Unique network node";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.rust-overlay = { url = "github:oxalica/rust-overlay"; flake = false; };
  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs
          {
            inherit system; overlays = [ (import rust-overlay) ];
          };
      in
      {
        devShell = pkgs.mkShell {
          nativeBuildInputs = [ pkgs.binutils pkgs.pkgconfig pkgs.openssl ((pkgs.rustChannelOf { date = "2021-08-16"; channel = "nightly"; }).default.override { extensions = [ "rust-src" ]; }) ];
        };
      });
}
