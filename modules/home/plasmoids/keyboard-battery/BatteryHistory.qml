import QtCore
import QtQml
import "BatteryTrend.js" as Trend

QtObject {
    readonly property int windowMs: Trend.windowMs
    readonly property int gapMs: Trend.gapMs
    readonly property var samples: {
        try {
            const data = JSON.parse(store.history);
            if (!Array.isArray(data) || !data.every(sample =>
                sample && Number.isFinite(sample.time) && sample.values && typeof sample.values === "object"))
                throw new Error("Invalid battery history");
            return data;
        } catch (error) {
            console.warn("Keyboard Battery: " + error);
            return [];
        }
    }

    property Settings store: Settings {
        location: StandardPaths.writableLocation(StandardPaths.GenericStateLocation)
            + "/mirsella-keyboard-battery.ini"
        property string history: "[]"
    }

    function record(now, values) {
        const updated = Trend.record(samples, now, values);
        if (updated !== samples)
            store.history = JSON.stringify(updated);
    }

    function estimate(key, now, level) {
        return Trend.estimate(samples, key, now, level);
    }
}
