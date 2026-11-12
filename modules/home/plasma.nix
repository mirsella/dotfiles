{
  config,
  isNixOS,
  lib,
  pkgs,
  ...
}:
let
  host = config.desktop.plasma;
  inherit (host.screen) width height;

  # Desktop widget grid on the primary screen: two halves on top, four
  # quarters below, snapped to the folder view's 16 px grid.
  snap = value: 16 * builtins.floor (value / 16.0 + 0.5);
  top = snap (height * 0.46);
  bottom = snap (height * 0.222);
  columnStart = column: snap (column * width / 4.0);
  desktopWidget = column: row: columns: widget:
    widget
    // {
      screen = 0;
      position = {
        horizontal = columnStart column;
        vertical = if row == 0 then 0 else top;
      };
      size = {
        width = columnStart (column + columns) - columnStart column;
        height = if row == 0 then top else bottom;
      };
    };

  # Removed widgets linger while the desktop script adds the new ones, so
  # Plasma shifts the new widgets aside, and it ignores later geometry
  # changes from scripts. The folder view reads ItemGeometries on load, so
  # write them before the restart below.
  widgetGeometry = builtins.listToAttrs (map (widget:
    lib.nameValuePair widget.name [
      widget.position.horizontal
      widget.position.vertical
      widget.size.width
      widget.size.height
    ]) config.programs.plasma.desktop.widgets);
  placeDesktopWidgets = ''
    {
      const desktop = desktops().filter((d) => d.screen === 0)[0];
      const geometry = ${builtins.toJSON widgetGeometry};
      const items = desktop.widgets()
        .filter((w) => geometry[w.type])
        .map((w) => "Applet-" + w.id + ":" + geometry[w.type].join(",") + ",0;")
        .join("");
      desktop.currentConfigGroup = [];
      desktop.writeConfig("ItemGeometries-${toString width}x${toString height}", items);
      desktop.writeConfig("ItemGeometriesHorizontal", items);
    }
  '';

  systemMonitor = plugin: title: config: {
    name = "org.kde.plasma.systemmonitor.${plugin}";
    config = lib.recursiveUpdate {
      CurrentPreset = "org.kde.plasma.systemmonitor";
      UserBackgroundHints = "StandardBackground";
      Appearance = {
        chartFace = "org.kde.ksysguard.linechart";
        inherit title;
      };
    } config;
  };

  cpuColors = builtins.listToAttrs (
    lib.imap0 (core: color: lib.nameValuePair "cpu/cpu${toString core}/usage" color) [
      "61,174,233" "61,142,233" "61,110,233" "61,77,233" "77,61,233" "109,61,233" "141,61,233"
      "174,61,233" "206,61,233" "233,61,228" "233,61,196" "233,61,163" "233,61,131" "233,61,99"
      "233,61,67" "233,88,61" "233,120,61" "233,152,61" "233,184,61" "233,217,61" "217,233,61"
      "185,233,61" "153,233,61" "120,233,61" "88,233,61" "61,233,66" "61,233,98" "61,233,131"
      "61,233,163" "61,233,195" "61,233,227" "61,206,233"
    ]
  );

  # The scripting API cannot reach applets inside the system tray, and
  # plasma-manager drops tray item settings. After the panel script has
  # created the tray, write them into its child groups.
  trayItemSettings = {
    "org.kde.plasma.battery".General.showPercentage = true;
    "org.kde.plasma.bluetooth".General.showNumberOfConnectedDevices = true;
    "org.kde.plasma.volume".General = {
      migrated = true;
      showVirtualDevices = true;
    };
    "org.kde.plasma.weather".WeatherStation = {
      placeDisplayName = "Fontainebleau, France, FR";
      placeInfo = "Fontainebleau, France, FR|3018074";
      provider = "bbcukmet";
    };
  };
  configureTrayItems = ''
    applets="${config.xdg.configHome}/plasma-org.kde.plasma.desktop-appletsrc"
    # plasmashell saves new applets a few seconds after creating them.
    for _ in $(seq 30); do
      grep -qx 'plugin=org.kde.plasma.weather' "$applets" && break
      sleep 1
    done
    # write PLUGIN GROUP KEY VALUE into the tray child running PLUGIN.
    write() {
      path=$(awk -v plugin="plugin=$1" '
        /^\[/ { group = $0 }
        $0 == plugin && group ~ /\]\[Applets\]\[[0-9]+\]\[Applets\]\[[0-9]+\]$/ { print group }
      ' "$applets")
      if [ -z "$path" ]; then
        echo "plasma-manager: tray item $1 not found; its settings were not applied" >&2
        return
      fi
      plugin=$1 group=$2 key=$3 value=$4
      set --
      for part in $(printf '%s' "$path" | sed 's/^\[//; s/\]$//; s/\]\[/ /g'); do
        set -- "$@" --group "$part"
      done
      kwriteconfig6 --file "$applets" "$@" --group Configuration --group "$group" --key "$key" "$value"
    }
    ${lib.concatStrings (lib.flatten (lib.mapAttrsToList (plugin: groups:
      lib.mapAttrsToList (group: entries:
        lib.mapAttrsToList (key: value: ''
          write ${plugin} ${group} ${key} ${lib.escapeShellArg (if builtins.isBool value then lib.boolToString value else toString value)}
        '') entries) groups) trayItemSettings))}
  '';
in
{
  options.desktop.plasma = {
    screen = {
      width = lib.mkOption {
        type = lib.types.ints.positive;
        description = "Logical width of the primary screen, for the desktop widget grid.";
      };
      height = lib.mkOption {
        type = lib.types.ints.positive;
        description = "Logical height of the primary screen, for the desktop widget grid.";
      };
    };
    cpuTemperature = {
      sensor = lib.mkOption {
        type = lib.types.str;
        description = "KSystemStats sensor for the CPU temperature.";
      };
      label = lib.mkOption {
        type = lib.types.str;
        description = "Legend label for the CPU temperature sensor.";
      };
    };
    rootDisk = lib.mkOption {
      type = lib.types.str;
      description = "Filesystem UUID of the root disk for the disk usage widget.";
    };
  };

  config = {
    # plasma-manager's login runner calls `qdbus`; Arch ships only `qdbus6`.
    home.packages = lib.optional (!isNixOS) (
      pkgs.writeShellScriptBin "qdbus" ''exec /usr/bin/qdbus6 "$@"''
    );

    programs.plasma = {
      enable = true;

      workspace = {
        # nasa-wallpaper replaces this image daily.
        wallpaper = "${config.xdg.cacheHome}/nasa-wallpaper/current.jpg";
        wallpaperFillMode = "preserveAspectCrop";
      };

      panels = [
        {
          location = "bottom";
          alignment = "center";
          lengthMode = "fit";
          height = 44;
          floating = true;
          hiding = "dodgewindows";
          widgets = [
            {
              name = "org.kde.plasma.kickoff";
              config.General = {
                compactMode = true;
                systemFavorites = "suspend\\,hibernate\\,reboot\\,shutdown";
              };
            }
            "org.kde.plasma.pager"
            {
              name = "org.kde.plasma.icontasks";
              config.General.launchers = [
                "applications:rio-cargo.desktop"
                "applications:zen.desktop"
                "applications:org.kde.dolphin.desktop"
                "applications:t3code.desktop"
                "applications:com.github.th-ch.youtube-music.desktop"
              ];
            }
            {
              name = "com.github.psimaker.codexbar";
              config.General = {
                cliPath = "~/.local/bin/codexbar";
                configMigrated = true;
                configSourcesMigrated = true;
                enabledProviders = "codex,opencodego,claude";
                providerSources = "opencodego=api";
                showApiCredits = true;
                openaiCreditProjectId = "proj_V8UqSU2ezu1cpyNYnAEZBxDJ";
                openaiCreditBudget = 100;
                hideCritters = true;
                panelDisplayMode = "logos";
                panelPercentSource = "weekly";
                separateIcons = true;
                showPercentInPanel = true;
              };
            }
            "org.kde.plasma.marginsseparator"
            {
              systemTray.items = {
                shown = [ "org.kde.plasma.battery" ];
                hidden = [ "org.kde.plasma.weather" ];
                extra = [
                  "org.kde.plasma.brightness"
                  "org.kde.plasma.cameraindicator"
                  "org.kde.plasma.mediacontroller"
                  "org.kde.plasma.keyboardindicator"
                  "org.kde.kscreen"
                  "org.kde.plasma.vault"
                  "org.kde.plasma.printmanager"
                  "org.kde.plasma.clipboard"
                  "org.kde.plasma.notifications"
                  "org.kde.plasma.devicenotifier"
                  "org.kde.plasma.keyboardlayout"
                  "org.kde.plasma.manage-inputmethod"
                  "org.kde.plasma.dwellclicker"
                  "org.kde.kupapplet"
                  "org.kde.plasma.bluetooth"
                  "org.kde.plasma.networkmanagement"
                  "org.kde.plasma.volume"
                  "org.kde.plasma.weather"
                  "org.kde.plasma.battery"
                  "org.kde.kdeconnect"
                ];
              };
            }
            {
              name = "org.kde.plasma.digitalclock";
              config.Appearance = {
                fontWeight = 400;
                showSeconds = "Always";
              };
            }
            "org.kde.plasma.showdesktop"
          ];
        }
      ];

      startup.desktopScript.finalize_layout = {
        text = placeDesktopWidgets;
        postCommands = configureTrayItems;
        restartServices = [ "plasma-plasmashell" ];
        # After the panels (2) and wallpaper (3) scripts.
        priority = 4;
      };

      desktop.widgets = [
        (desktopWidget 0 0 2 {
          name = "org.mirsella.memory";
          config.UserBackgroundHints = "StandardBackground";
        })
        (desktopWidget 2 0 2 (systemMonitor "cpucore" "Individual Core Usage" {
          SensorColors = cpuColors // { ${host.cpuTemperature.sensor} = "255,255,255"; };
          SensorLabels.${host.cpuTemperature.sensor} = host.cpuTemperature.label;
          Sensors = {
            highPrioritySensorIds = builtins.toJSON [ "cpu/cpu.*/usage" host.cpuTemperature.sensor ];
            lowPrioritySensorIds = builtins.toJSON [
              "cpu/all/usage"
              "cpu/all/averageFrequency"
              host.cpuTemperature.sensor
            ];
            totalSensors = builtins.toJSON [ "cpu/all/usage" ];
          };
          "org.kde.ksysguard.linechart/General" = {
            rangeAutoY = false;
            rangeFromY = 0;
            rangeToY = 100;
          };
        }))
        (desktopWidget 0 1 1 (systemMonitor "net" "Network Speed" {
          Appearance.showTitle = true;
          SensorColors = {
            "network/all/download" = "61,174,233";
            "network/all/upload" = "233,120,61";
          };
          Sensors.highPrioritySensorIds = builtins.toJSON [ "network/all/download" "network/all/upload" ];
        }))
        (desktopWidget 1 1 1 (systemMonitor "diskactivity" "Hard Disk Activity" {
          Appearance.showTitle = true;
          SensorColors = {
            "disk/all/read" = "233,120,61";
            "disk/all/write" = "61,174,233";
          };
          Sensors.highPrioritySensorIds = builtins.toJSON [ "disk/all/write" "disk/all/read" ];
        }))
        (desktopWidget 2 1 1 (systemMonitor "diskusage" "Disk Usage" {
          updateRateLimit = 60000;
          Appearance = {
            historyAmount = 86400;
            showLegend = true;
            showTitle = false;
          };
          SensorColors."disk/${host.rootDisk}/usedPercent" = "61,174,233";
          SensorLabels = {
            "disk/${host.rootDisk}/free" = "Free";
            "disk/${host.rootDisk}/usedPercent" = "Used";
          };
          Sensors = {
            highPrioritySensorIds = builtins.toJSON [ "disk/${host.rootDisk}/usedPercent" ];
            lowPrioritySensorIds = "[]";
            totalSensors = "[]";
          };
        }))
        (desktopWidget 3 1 1 {
          name = "com.github.zren.commandoutput";
          config = {
            UserBackgroundHints = "StandardBackground";
            General = {
              command = "${config.home.homeDirectory}/.local/bin/zmk_split_battery_level.py";
              fontFamily = "JetBrainsMono Nerd Font";
              interval = 30000;
              outlineColor = "#000000";
              showBackground = true;
              showOutline = false;
              textColor = "#000000";
              waitForCompletion = true;
            };
          };
        })
      ];

      configFile = {
        kcminputrc = {
          "Libinput/1133/16505/Logitech G Pro ".PointerAccelerationProfile = 1;
          "Libinput/9610/51/Glorious Model D" = {
            PointerAccelerationProfile = 2;
            ScrollMethod = 0;
          };
        };

        krunnerrc = {
          General = {
            FreeFloating = true;
            historyBehavior = "ImmediateCompletion";
          };
          Plugins.baloosearchEnabled = true;
          "Plugins/Favorites".plugins = "krunner_services";
        };

        kscreenlockerrc.Daemon = {
          Autolock = false;
          Timeout = 0;
        };

        kxkbrc.Layout = {
          DisplayNames = ",col";
          LayoutDefaultGlobal = 1;
          LayoutList = "us,us";
          Use = true;
          VariantList = ",colemak_dh_iso";
        };

        plasmarc.AdaptiveTransparency.enabled = false;
        plasmashellrc.General."Number of Actions" = 0;

        powerdevilrc = {
          "AC/Display" = {
            DimDisplayIdleTimeoutSec = 900;
            TurnOffDisplayIdleTimeoutSec = 1800;
            TurnOffDisplayIdleTimeoutWhenLockedSec = 60;
            TurnOffDisplayWhenIdle = true;
          };
          "AC/SuspendAndShutdown" = {
            AutoSuspendAction = 0;
            InhibitLidActionWhenExternalMonitorPresent = false;
          };
          "Battery/Display" = {
            DimDisplayIdleTimeoutSec = 300;
            TurnOffDisplayIdleTimeoutSec = 360;
            TurnOffDisplayIdleTimeoutWhenLockedSec = 60;
          };
          "Battery/SuspendAndShutdown" = {
            InhibitLidActionWhenExternalMonitorPresent = false;
            SleepMode = 3;
          };
          BatteryManagement.BatteryCriticalLevel = 4;
          "LowBattery/Display" = {
            DimDisplayIdleTimeoutSec = 120;
            TurnOffDisplayIdleTimeoutSec = 180;
            TurnOffDisplayIdleTimeoutWhenLockedSec = 60;
            UseProfileSpecificDisplayBrightness = false;
          };
          "LowBattery/Performance".PowerProfile = "power-saver";
          "LowBattery/SuspendAndShutdown" = {
            InhibitLidActionWhenExternalMonitorPresent = false;
            SleepMode = 3;
          };
        };

        kwinrc = {
          # Tiling layouts are keyed by these desktop IDs.
          Desktops = {
            Id_1 = "1bc4a98b-9aa0-4dcb-8003-afd3b178df4c";
            Id_2 = "62394944-6f6a-408a-88c4-aaf4c5e4debe";
            Id_3 = "a0137979-a340-49e5-bc17-7f61aacacead";
            Number = 3;
            Rows = 1;
          };
          # Custom tile layouts for main's two monitors on desktops 1 and 2.
          "Tiling/1bc4a98b-9aa0-4dcb-8003-afd3b178df4c/674468cb-3b8d-4f4c-a3af-d89aa03d152f" = {
            padding = 0;
            tiles = ''{"layoutDirection":"horizontal","tiles":[{"layoutDirection":"vertical","tiles":[{"height":0.24920244567333869},{"height":0.24920244567333869},{"height":0.25079755432666123},{"height":0.2507975543266612}],"width":1}]}'';
          };
          "Tiling/1bc4a98b-9aa0-4dcb-8003-afd3b178df4c/bf6df61f-9e97-424b-9ff9-6554e5472e3f" = {
            padding = 0;
            tiles = ''{"layoutDirection":"horizontal","tiles":[{"width":0.22082088370682418},{"width":0.5473824701543538},{"width":0.231796646138822}]}'';
          };
          "Tiling/62394944-6f6a-408a-88c4-aaf4c5e4debe/674468cb-3b8d-4f4c-a3af-d89aa03d152f" = {
            padding = 0;
            tiles = ''{"layoutDirection":"horizontal","tiles":[{"layoutDirection":"vertical","tiles":[{"height":0.25},{"height":0.25},{"height":0.25},{"height":0.25}],"width":1}]}'';
          };
          "Tiling/62394944-6f6a-408a-88c4-aaf4c5e4debe/bf6df61f-9e97-424b-9ff9-6554e5472e3f".padding = 0;
          "Effect-zoom".UsePatternUpscaler = false;
          NightColor = {
            Active = true;
            NightTemperature = 5000;
          };
          Plugins = {
            helium-attention-silencerEnabled = true;
            synchronizeskipswitcherEnabled = true;
          };
          Wayland.EnablePrimarySelection = false;
          Windows.SeparateScreenFocus = false;
          Windows_HDR = {
            MaxFrameAverage = 0;
            MaxLuminance = 390;
            Reference = 390;
          };
          Xwayland = {
            Scale = 1;
            XwaylandEisNoPromptApps = "xdotool";
          };
        };

        kwinrulesrc = {
          General.Order = "fc9c0a8c-85d7-4dba-a2b6-35075c9f989b";
          "fc9c0a8c-85d7-4dba-a2b6-35075c9f989b" = {
            Description = "Window settings for Qemu-system-x86_64";
            clientmachine = "a1ed95728936";
            desktops = "ae82677a-1d7b-4270-bae3-9bc907533d7a";
            desktopsrule = 3;
            title = "QEMU";
            types = 1;
            wmclass = "qemu Qemu-system-x86_64";
            wmclasscomplete = true;
            wmclassmatch = 1;
          };
        };
      };
    };
  };
}
