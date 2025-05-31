self:
{
  config,
  pkgs,
  lib,
  ...
}:
let
  inherit (lib)
    mkIf
    getExe
    ;
  inherit (lib.options) mkOption mkEnableOption;
  inherit (lib.types)
    nullOr
    listOf
    str
    bool
    package
    submodule
    ;

  cfg = config.services.kidex;
in
{
  meta.maintainers = with lib.maintainers; [ Kirottu ];

  options.services.kidex = {
    enable = mkEnableOption "kidex";
    package = mkOption {
      type = nullOr package;
      default = self.packages.${pkgs.system}.kidex;
    };
    settings = {
      ignored = mkOption {
        type = listOf str;
        description = ''
          Global ignore patterns for the indexer
        '';
        default = [ ];
      };
      directories = mkOption {
        type = listOf (submodule {
          options = {
            path = mkOption {
              type = str;
              description = ''
                Path to index
              '';
            };
            recurse = mkOption {
              type = bool;
              description = ''
                Whether to recurse further into the directory
              '';
            };
            ignored = mkOption {
              type = listOf str;
              description = ''
                Ignore patterns for this specific directory
              '';
              default = [ ];
            };
          };
        });
        description = ''
          Directories to index, and their configurations
        '';
        default = [ ];
      };
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      (lib.hm.assertions.assertPlatform "services.kidex" pkgs lib.platforms.linux)
    ];

    home.packages = mkIf (cfg.package != null) [ cfg.package ];

    systemd.user.services.kidex = mkIf (cfg.package != null) {
      Unit = {
        Description = "A simple file indexing service";
        # This resolves to "graphical-session.target", so should work across any user sessions
        PartOf = [ config.wayland.systemd.target ];
        After = [ config.wayland.systemd.target ];
      };
      Service = {
        Type = "simple";
        ExecStart = "${getExe cfg.package}";
        Restart = "on-failure";
      };
      Install.WantedBy = [ config.wayland.systemd.target ];
    };

    xdg.configFile."kidex.ron" = {
      onChange = "${cfg.package}/bin/kidex-client reload-config";
      text = ''
        Config(
          ignored: [${lib.concatStrings (builtins.map (x: "\"${x}\",") cfg.settings.ignored)}],
          directories: [${
            lib.concatStrings (
              builtins.map (watch-dir: ''
                WatchDir(
                  path: "${watch-dir.path}",
                  recurse: ${lib.boolToString watch-dir.recurse},
                  ignored: [${lib.concatStrings (builtins.map (x: "\"${x}\",") watch-dir.ignored)}],
                ),
              '') cfg.settings.directories
            )
          }]
        )
      '';
    };
  };
}
