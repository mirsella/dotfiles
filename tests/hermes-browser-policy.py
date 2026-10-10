import json
import os
from pathlib import Path
import sys
import tempfile


with tempfile.TemporaryDirectory() as directory:
    home = Path(directory)
    os.environ["HERMES_HOME"] = directory
    os.environ["CAMOFOX_URL"] = "http://127.0.0.1:9378"
    os.environ["CAMOFOX_USER_ID"] = "home-browser"
    os.environ["CAMOFOX_ADOPT_EXISTING_TAB"] = "false"
    os.environ.pop("CAMOFOX_SESSION_KEY", None)
    plugin = home / "plugins" / "camofox-accounts"
    plugin.parent.mkdir()
    plugin.symlink_to(Path(sys.argv[1]).resolve(), target_is_directory=True)
    (home / "config.yaml").write_text(
        json.dumps(
            {
                "browser": {"backend": "browserbase", "cloud_provider": "camofox"},
                "plugins": {"enabled": ["camofox-accounts"]},
            }
        )
    )

    from model_tools import get_tool_definitions
    from tools import browser_camofox
    from hermes_cli.plugins import get_plugin_manager
    from toolsets import resolve_toolset

    manager = get_plugin_manager()
    manager.discover_and_load()
    assert any(
        p["name"] == "camofox-accounts" and p["enabled"] and not p["error"]
        for p in manager.list_plugins()
    ), manager.list_plugins()

    parent = browser_camofox._get_session("parent-task")
    child = browser_camofox._get_session("subagent-task")
    assert parent["user_id"] == child["user_id"] == "home-browser"
    assert parent["session_key"] != child["session_key"]
    assert parent["managed"] and child["managed"]
    assert not parent["adopt_existing_tab"] and not child["adopt_existing_tab"]
    assert browser_camofox.camofox_soft_cleanup("parent-task")
    assert browser_camofox._get_session("subagent-task") is child

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
    vault = set(resolve_toolset("browser-vault-cdp"))
    assert vault == {name for name in available if name.startswith("browser_vault_")}
    assert "browser_vault_fill" in vault
    restricted = names(["browser-cdp", "browser-use", "browser-vault-cdp"])
    assert {
        "browser_navigate",
        "browser_snapshot",
        "browser_type",
        "browser_console",
    } <= restricted
    assert not restricted & {
        "browser_cdp",
        "browser_exec",
        "browser_dialog",
    }
    assert restricted == available - vault - {
        "browser_cdp",
        "browser_exec",
        "browser_dialog",
    }
    print(
        "Native Camofox tools keep JavaScript and task isolation without unsupported CDP vault tools"
    )
