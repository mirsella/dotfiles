import json
import os
from pathlib import Path
import sys
import tempfile


with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    os.environ["HERMES_HOME"] = directory
    os.environ["CAMOFOX_URL"] = "http://127.0.0.1:9378"
    (home / "plugins").mkdir()
    (home / "plugins/browser-policy").symlink_to(sys.argv[1])
    (home / "config.yaml").write_text(
        json.dumps(
            {
                "browser": {"backend": "browserbase", "cloud_provider": "camofox"},
                "plugins": {"enabled": ["browser-policy"]},
            }
        )
    )

    from hermes_cli.plugins import get_plugin_manager
    from model_tools import get_tool_definitions
    from toolsets import resolve_toolset

    manager = get_plugin_manager()
    manager.discover_and_load()
    plugin = next(p for p in manager.list_plugins() if p["name"] == "browser-policy")
    assert plugin["enabled"] and not plugin["error"], plugin
    assert resolve_toolset("browser-evaluation") == ["browser_console"]

    def names(disabled):
        schemas = get_tool_definitions(
            enabled_toolsets=["browser"],
            disabled_toolsets=disabled,
            quiet_mode=True,
            skip_tool_search_assembly=True,
        )
        return {schema["function"]["name"] for schema in schemas}

    available = names([])
    assert "browser_console" in available, available
    restricted = names(["browser-cdp", "browser-use", "browser-evaluation"])
    assert {"browser_navigate", "browser_snapshot", "browser_type"} <= restricted
    assert not restricted & {
        "browser_console",
        "browser_cdp",
        "browser_exec",
        "browser_dialog",
    }
    assert restricted == available - {
        "browser_console",
        "browser_cdp",
        "browser_exec",
        "browser_dialog",
    }
    print(
        "Native browser policy loads and hides unsupported tools without adding tools"
    )
