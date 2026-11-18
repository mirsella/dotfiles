pragma ComponentBehavior: Bound

import org.kde.plasma.plasmoid

PlasmoidItem {
    id: applet
    preferredRepresentation: fullRepresentation

    fullRepresentation: Monitor {
        id: monitor
        command: applet.plasmoid.configuration.command
        title: "Keyboard Battery"
        currentLabel: "14 days"
        oldestLabel: "−14 d"
        windowMs: monitor.batteryHistory.windowMs
        batteryHistory: BatteryHistory {}
        series: [
            { key: "Battery Level", label: "Central", color: dark ? "#b5a0ff" : "#7147ac" },
            { key: "Peripheral 0", label: "Peripheral", color: dark ? "#60d6d5" : "#087a83" }
        ]
    }
}
