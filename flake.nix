{
  description = "Kagi (鍵) — GPU-rendered 1Password client";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-25.11";
    crate2nix.url = "github:nix-community/crate2nix";
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crate2nix,
    flake-utils,
    substrate,
  }:
    (import "${substrate}/lib/rust-tool-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils;
    }) {
      toolName = "kagi";
      src = self;
      repo = "pleme-io/kagi";
      module = {
        description = "Kagi — GPU-rendered 1Password client";
        hmNamespace = "blackmatter.components";

        extraHmOptions = {
          connect = {
            url = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.str;
              default = null;
              description = "1Password Connect server URL.";
              example = "http://localhost:8080";
            };
            tokenCommand = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.str;
              default = null;
              description = "Command to retrieve the 1Password Connect token.";
              example = "cat /run/secrets/op-connect-token";
            };
          };
          appearance = {
            width = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.int;
              default = null;
              description = "Window width in pixels.";
            };
            height = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.int;
              default = null;
              description = "Window height in pixels.";
            };
            background = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.str;
              default = null;
              description = "Background color (hex).";
            };
            foreground = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.str;
              default = null;
              description = "Foreground color (hex).";
            };
            accent = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.str;
              default = null;
              description = "Accent color (hex).";
            };
          };
          clipboard = {
            autoClearSecs = nixpkgs.lib.mkOption {
              type = nixpkgs.lib.types.nullOr nixpkgs.lib.types.int;
              default = null;
              description = "Auto-clear clipboard after N seconds (0 to disable).";
              example = 30;
            };
          };
          extraSettings = nixpkgs.lib.mkOption {
            type = nixpkgs.lib.types.attrs;
            default = { };
            description = ''
              Additional raw settings merged on top of typed options.
              Values are serialized directly to YAML.
            '';
          };
        };

        # Render YAML at ~/.config/kagi/kagi.yaml — shape-preserving with the
        # original module: filter nulls, snake_case the wire format, merge
        # extraSettings on top.
        extraHmConfig = cfg: {
          xdg.configFile."kagi/kagi.yaml".text =
            let
              filterNulls = nixpkgs.lib.filterAttrs (_: v: v != null);
              settingsAttr = (filterNulls {
                connect = filterNulls {
                  url = cfg.connect.url;
                  token_command = cfg.connect.tokenCommand;
                };
                appearance = filterNulls {
                  width = cfg.appearance.width;
                  height = cfg.appearance.height;
                  background = cfg.appearance.background;
                  foreground = cfg.appearance.foreground;
                  accent = cfg.appearance.accent;
                };
                clipboard = filterNulls {
                  auto_clear_secs = cfg.clipboard.autoClearSecs;
                };
              }) // cfg.extraSettings;
            in
              nixpkgs.lib.generators.toYAML { } settingsAttr;
        };
      };
    };
}
