from toolsets import create_custom_toolset


def register(ctx):
    create_custom_toolset(
        "browser-unsupported",
        "Browser tools unavailable through the shared Camofox controller.",
        tools=[
            "browser_console",
            "browser_vault_list",
            "browser_vault_unlock",
            "browser_vault_fill",
            "browser_vault_save_login",
            "browser_vault_enter_code",
        ],
        includes=["browser-cdp", "browser-use"],
    )
