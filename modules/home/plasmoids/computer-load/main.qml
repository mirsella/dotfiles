pragma ComponentBehavior: Bound

import org.kde.plasma.plasmoid

PlasmoidItem {
    id: applet
    preferredRepresentation: fullRepresentation

    fullRepresentation: Monitor {
        command: applet.plasmoid.configuration.command
        title: "Computer Load"
        currentLabel: "30 min"
        oldestLabel: "−30 min"
        windowMs: 30 * 60 * 1000
        series: [
            { key: "main", label: "main", color: dark ? "#72c5ff" : "#126ba6" },
            { key: "laptop", label: "laptop", color: dark ? "#f9ad76" : "#ad4f1d" },
            { key: "predator", label: "predator", color: dark ? "#79d4ad" : "#087f61" }
        ]
    }
}
