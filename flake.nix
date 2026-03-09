{
  description = "Kagi (鍵) — GPU-rendered 1Password client";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      substrate,
      ...
    }:
    let
      system = "aarch64-darwin";
      pkgs = import nixpkgs { inherit system; };

      mkDate =
        longDate:
        (nixpkgs.lib.concatStringsSep "-" [
          (builtins.substring 0 4 longDate)
          (builtins.substring 4 2 longDate)
          (builtins.substring 6 2 longDate)
        ]);

      props = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      version =
        props.package.version
        + "+date="
        + (mkDate (self.lastModifiedDate or "19700101"))
        + "_"
        + (self.shortRev or "dirty");

      pname = "kagi";

      package = pkgs.rustPlatform.buildRustPackage {
        inherit pname version;
        src = pkgs.lib.cleanSource ./.;
        cargoLock.lockFile = ./Cargo.lock;
        doCheck = false;
        meta.mainProgram = pname;
      };
    in
    {
      packages.${system} = {
        kagi = package;
        default = package;
      };

      overlays.default = final: prev: {
        kagi = self.packages.${final.system}.default;
      };

      homeManagerModules.default = import ./module {
        hmHelpers = import "${substrate}/lib/hm-service-helpers.nix" { lib = nixpkgs.lib; };
      };

      devShells.${system}.default = pkgs.mkShellNoCC {
        packages = [
          pkgs.rustc
          pkgs.cargo
          pkgs.rust-analyzer
          pkgs._1password-cli
        ];
      };

      formatter.${system} = pkgs.nixfmt-tree;
    };
}
