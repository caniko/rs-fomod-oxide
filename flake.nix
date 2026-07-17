{
  inputs = {
    rs-harbor.url = "git+https://codeberg.org/caniko/rs-harbor.git?ref=trunk&rev=9bfa8bdb0ecb22d7bc11448665f7fbaebae7a759";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    ronix.url = "git+https://codeberg.org/caniko/ronix";
    plinth = {
      url = "git+https://codeberg.org/caniko/plinth.git?ref=refs/heads/trunk";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      rs-harbor,
      nixpkgs,
      crane,
      flake-utils,
      ronix,
      plinth,
      rust-overlay,
      ...
    }:
    {
      lib.ronix = ronix.lib;
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        buildCache = rs-harbor.lib.mkBuildCachePolicy {
          inherit pkgs;
          sccachePackage = rs-harbor.packages.${system}.sccache;
          cacheRoot = null;
          namespaceScope = "canix-rust";
          namespaceGeneration = 5;
        };

        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            (pkgs.lib.hasSuffix ".xml" path) || (craneLib.filterCargoSources path type);
        };

        commonArgs = {
          inherit src;
          strictDeps = true;
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        fomod-oxide = buildCache.withRustCache { package = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
          }
        ); };
        website = plinth.lib.${system}.mkProjectSite {
          pname = "fomod-oxide-website";
          domain = "fomod-oxide.tartanoglu.com";
          configPath = ./website/plinth-project.toml;
        };
      in
      {
        checks = {
          inherit fomod-oxide;

          fomod-oxide-clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );

          fomod-oxide-fmt = craneLib.cargoFmt { inherit src; };
        };

        packages = {
          default = fomod-oxide;
          website = website;
          site = website;
        };

        apps.deploy-pages = plinth.lib.${system}.mkDeployPagesApp {
          domain = "fomod-oxide.tartanoglu.com";
        };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};

          packages = with pkgs; [
            cargo-edit
            cargo-release
          ];
        };
      }
    );
}
