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
    unsupported = {
        "browser_console",
        "browser_cdp",
        "browser_exec",
        "browser_dialog",
        "browser_vault_list",
        "browser_vault_unlock",
        "browser_vault_fill",
        "browser_vault_save_login",
        "browser_vault_enter_code",
    }
    assert set(resolve_toolset("browser-unsupported")) == unsupported

    def tools_for(toolset, disabled=None):
        return {
            schema["function"]["name"]: schema["function"]
            for schema in get_tool_definitions(
                enabled_toolsets=[toolset],
                disabled_toolsets=disabled,
                quiet_mode=True,
                skip_tool_search_assembly=True,
            )
        }

    available = set(tools_for("browser"))
    assert {"browser_console", "browser_vault_fill"} <= available, available
    supported = available - unsupported
    assert {"browser_navigate", "browser_snapshot", "browser_type"} <= supported
    for toolset in ["browser", "hermes-cli", "hermes-telegram"]:
        tools = tools_for(toolset, ["browser-unsupported"])
        assert not tools.keys() & unsupported, (toolset, tools.keys() & unsupported)
        assert tools.keys() & available == supported, (toolset, tools.keys() & available)
        for name in supported:
            description = tools[name]["description"]
            assert "Vault note:" not in description, (toolset, name, description)
            assert "Never type a password" not in description, (toolset, name, description)
    print("Camofox tools exclude the CDP vault and allow direct credential entry")
