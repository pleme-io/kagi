# Kagi home-manager module — GPU-rendered 1Password client
#
# Namespace: blackmatter.components.kagi.*
#
# Generates YAML config from typed Nix options, loaded by shikumi at runtime.
#
# Module factory: receives { hmHelpers } from flake.nix, returns HM module.
{ hmHelpers }:
{
  lib,
  config,
  pkgs,
  ...
}:
with lib;
let
  cfg = config.blackmatter.components.kagi;

  settingsAttr =
    let
      filterNulls = filterAttrs (_: v: v != null);
    in
    filterNulls {
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
    }
    // cfg.extraSettings;

  yamlConfig = pkgs.writeText "kagi.yaml" (
    lib.generators.toYAML { } settingsAttr
  );
in
{
  options.blackmatter.components.kagi = {
    enable = mkEnableOption "Kagi — GPU-rendered 1Password client";

    package = mkOption {
      type = types.package;
      default = pkgs.kagi;
      description = "The kagi package to use.";
    };

    # 1Password Connect API
    connect = {
      url = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "1Password Connect server URL.";
        example = "http://localhost:8080";
      };

      tokenCommand = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Command to retrieve the 1Password Connect token.";
        example = "cat /run/secrets/op-connect-token";
      };
    };

    # Appearance
    appearance = {
      width = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Window width in pixels.";
      };

      height = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Window height in pixels.";
      };

      background = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Background color (hex).";
      };

      foreground = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Foreground color (hex).";
      };

      accent = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Accent color (hex).";
      };
    };

    # Clipboard
    clipboard = {
      autoClearSecs = mkOption {
        type = types.nullOr types.int;
        default = null;
        description = "Auto-clear clipboard after N seconds (0 to disable).";
        example = 30;
      };
    };

    # Escape hatch
    extraSettings = mkOption {
      type = types.attrs;
      default = { };
      description = ''
        Additional raw settings merged on top of typed options.
        Values are serialized directly to YAML.
      '';
    };
  };

  config = mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile."kagi/kagi.yaml".source = yamlConfig;
  };
}
