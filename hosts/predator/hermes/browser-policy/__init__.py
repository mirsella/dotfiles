from toolsets import create_custom_toolset


def register(ctx):
    create_custom_toolset(
        "browser-evaluation",
        "Browser evaluation is unavailable through the shared Camofox controller.",
        tools=["browser_console"],
    )
