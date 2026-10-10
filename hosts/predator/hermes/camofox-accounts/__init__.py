from toolsets import create_custom_toolset, resolve_toolset


def register(ctx):
    create_custom_toolset(
        "browser-vault-cdp",
        "Browser-vault autofill requires CDP; Camofox uses the Pass CLI bridge.",
        tools=[
            name
            for name in resolve_toolset("browser")
            if name.startswith("browser_vault_")
        ],
    )
