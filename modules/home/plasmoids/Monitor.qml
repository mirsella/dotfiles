pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasma5support as Plasma5Support

Item {
    id: root
    Layout.minimumWidth: 230
    Layout.minimumHeight: 150

    required property string command
    required property string title
    required property string currentLabel
    required property string oldestLabel
    required property int windowMs
    required property var series
    property var batteryHistory: null
    readonly property bool battery: batteryHistory !== null
    readonly property color textColor: palette.windowText
    readonly property bool dark: textColor.hslLightness > 0.5
    readonly property int pollMs: 30000
    readonly property int smallTextSize: Math.max(10, Math.round(textMetrics.height * 0.72))

    // Avoid the Plasma Kirigami style's attached theme objects here.
    // Their palette updates have crashed plasmashell during window attachment.
    SystemPalette { id: palette }
    FontMetrics { id: textMetrics }

    component MonitorLabel: Text {
        color: root.textColor
        textFormat: Text.PlainText
    }

    property var loadHistory: []
    readonly property var history: battery
        ? batteryHistory.samples.concat({ time: now, values: current })
        : loadHistory
    property var previousZeroes: []
    property var current: ({})
    property string status: "Collecting…"
    property bool running: false
    property double now: Date.now()
    readonly property real ceiling: {
        if (battery) return 100;
        let peak = 0;
        for (const sample of history) {
            for (const item of series) peak = Math.max(peak, sample.values[item.key] ?? 0);
        }
        return Math.max(150, 50 * Math.ceil(peak / 50));
    }

    function poll() {
        if (command && !running) {
            running = true;
            sampler.connectSource(command);
        }
    }

    function accept(data) {
        let values = {};
        try {
            if (data["exit code"] !== 0 || data["exit status"] !== 0)
                throw new Error(data["stderr"] || "Sampler failed");
            const payload = JSON.parse(data["stdout"]);
            if (payload === null && battery) {
                status = "Disconnected";
            } else {
                if (!payload || typeof payload !== "object" || Array.isArray(payload))
                    throw new Error("Invalid sample payload");
                for (const item of series) {
                    const value = payload[item.key];
                    if (value == null) continue;
                    if (!Number.isFinite(value) || value < 0 || (battery && value > 100))
                        throw new Error("Invalid sample: " + item.key);
                    values[item.key] = value;
                }
                status = Object.keys(values).length > 0 ? ""
                    : (battery ? "Waiting for batteries" : "No hosts reachable");
            }
        } catch (error) {
            console.warn(root.title + ": " + error);
            status = "Read failed";
            values = {};
        }
        if (battery) {
            // The peripheral sometimes reports a single 0% when it wakes.
            // Confirm zero before plotting it or calling the half depleted.
            const zeroes = Object.keys(values).filter(key => values[key] === 0);
            for (const key of zeroes) {
                if (!previousZeroes.includes(key))
                    delete values[key];
            }
            previousZeroes = zeroes;
        }
        now = Date.now();
        current = values;
        if (battery) {
            batteryHistory.record(now, values);
        } else {
            loadHistory = loadHistory.filter(sample => sample.time >= now - windowMs && sample.time < now)
                .concat({ time: now, values: values });
        }
    }

    function duration(hours) {
        if (hours < 1 / 60)
            return { amount: Math.round(hours * 3600), unit: "s" };
        if (hours < 1)
            return { amount: Math.round(hours * 60), unit: "min" };
        if (hours < 24)
            return { amount: Math.round(hours), unit: "h" };
        return { amount: Math.round(hours / 24), unit: "d" };
    }

    function forecastRange(forecast) {
        const low = duration(forecast.lowHours);
        const lower = low.amount + " " + low.unit;
        if (forecast.highHours === null)
            return "≥" + lower;
        const high = duration(forecast.highHours);
        if (low.unit !== high.unit)
            return lower + " – " + high.amount + " " + high.unit;
        if (low.amount === high.amount)
            return "≈" + lower;
        return low.amount + "–" + high.amount + " " + high.unit;
    }

    Plasma5Support.DataSource {
        id: sampler
        engine: "executable"
        connectedSources: []
        onNewData: (source, data) => {
            disconnectSource(source);
            root.running = false;
            root.accept(data);
        }
    }

    Timer {
        interval: root.pollMs
        repeat: true
        running: root.command.length > 0
        triggeredOnStart: true
        onTriggered: root.poll()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: root.battery ? 5 : 7

        RowLayout {
            Layout.fillWidth: true
            MonitorLabel {
                Layout.fillWidth: true
                elide: Text.ElideRight
                font.pixelSize: Math.round(textMetrics.height * 1.15)
                font.weight: Font.DemiBold
                text: root.title
            }
            MonitorLabel {
                font.pixelSize: root.smallTextSize
                opacity: 0.75
                text: root.status || root.currentLabel
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 5
            Repeater {
                model: root.series
                delegate: ColumnLayout {
                    required property var modelData
                    readonly property var reading: root.current[modelData.key]
                    readonly property var forecast: root.battery
                        ? root.batteryHistory.estimate(modelData.key, root.now, reading) : null
                    Layout.fillWidth: true
                    Layout.horizontalStretchFactor: 1
                    Layout.alignment: Qt.AlignTop
                    spacing: 2

                    RowLayout {
                        visible: !root.battery
                        spacing: 5
                        Rectangle {
                            Layout.preferredWidth: 7
                            Layout.preferredHeight: 7
                            radius: 3.5
                            color: modelData.color
                        }
                        MonitorLabel {
                            Layout.fillWidth: true
                            text: modelData.label
                            elide: Text.ElideRight
                        }
                    }
                    Row {
                        id: readingRow
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.max(batteryLabel.implicitHeight, percentage.implicitHeight)
                        spacing: 4
                        MonitorLabel {
                            id: batteryLabel
                            visible: root.battery
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.max(0, Math.min(Math.ceil(implicitWidth),
                                readingRow.width - percentage.width - readingRow.spacing))
                            text: modelData.label
                            elide: Text.ElideRight
                        }
                        MonitorLabel {
                            id: percentage
                            anchors.verticalCenter: parent.verticalCenter
                            text: reading == null ? "—" : Math.round(reading) + "%"
                            color: modelData.color
                            font.bold: true
                            font.pixelSize: root.battery ? textMetrics.height : Math.round(textMetrics.height * 1.22)
                        }
                        Item {
                            readonly property real availableWidth: readingRow.width
                                - batteryLabel.width - percentage.width - 2 * readingRow.spacing
                            visible: root.battery && availableWidth >= 24
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.max(0, Math.min(58, availableWidth))
                            height: 11
                            opacity: reading == null ? 0.45 : 1
                            Rectangle {
                                anchors.fill: parent
                                anchors.rightMargin: 3
                                radius: 2
                                border.color: modelData.color
                                color: "transparent"
                                Rectangle {
                                    x: 2
                                    y: 2
                                    width: Math.max(0, (parent.width - 4) * ((reading ?? 0) / 100))
                                    height: parent.height - 4
                                    radius: 1
                                    color: modelData.color
                                }
                            }
                            Rectangle {
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                width: 2
                                height: 5
                                radius: 1
                                color: modelData.color
                            }
                        }
                    }
                    MonitorLabel {
                        visible: root.battery && reading != null && reading > 0
                        text: forecast ? root.forecastRange(forecast) + " left" : "Waiting for a drop…"
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pixelSize: root.smallTextSize
                        opacity: 0.8
                    }
                }
            }
        }

        Item {
            id: graph
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 42

            MonitorLabel {
                id: topTick
                text: root.ceiling + "%"
                font.pixelSize: root.smallTextSize
                opacity: 0.75
            }
            MonitorLabel {
                anchors.bottom: parent.bottom
                text: "0%"
                font.pixelSize: root.smallTextSize
                opacity: 0.75
            }
            Canvas {
                id: plot
                anchors.fill: parent
                anchors.leftMargin: topTick.width + 7
                anchors.topMargin: topTick.height / 2
                anchors.bottomMargin: topTick.height / 2
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()

                Connections {
                    target: root
                    function onHistoryChanged() { plot.requestPaint(); }
                    function onSeriesChanged() { plot.requestPaint(); }
                    function onTextColorChanged() { plot.requestPaint(); }
                }

                onPaint: {
                    const ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);
                    function y(value) { return 3 + (height - 6) * (1 - value / root.ceiling); }
                    function grid(value, dashed) {
                        ctx.strokeStyle = Qt.alpha(root.textColor, dashed ? 0.42 : 0.20);
                        ctx.lineWidth = 1;
                        ctx.setLineDash(dashed ? [4, 4] : []);
                        ctx.beginPath();
                        ctx.moveTo(0, y(value));
                        ctx.lineTo(width, y(value));
                        ctx.stroke();
                    }
                    grid(0, false);
                    grid(root.ceiling, false);
                    grid(root.battery ? 50 : 100, true);
                    ctx.setLineDash([]);

                    const gap = root.battery ? root.batteryHistory.gapMs : 2 * root.pollMs;
                    for (const item of root.series) {
                        ctx.strokeStyle = item.color;
                        ctx.fillStyle = item.color;
                        ctx.lineWidth = 2;
                        ctx.lineCap = "round";
                        ctx.beginPath();
                        let previous = null;
                        let latest = null;
                        for (const sample of root.history) {
                            if (sample.time < root.now - root.windowMs || sample.time > root.now)
                                continue;
                            const value = sample.values[item.key];
                            if (value == null) {
                                previous = null;
                                continue;
                            }
                            const x = 3 + (width - 6) * (1 + (sample.time - root.now) / root.windowMs);
                            const point = { x: x, y: y(value), time: sample.time };
                            if (previous && point.time - previous.time < gap) {
                                ctx.lineTo(point.x, point.y);
                            } else {
                                ctx.moveTo(point.x, point.y);
                                if (root.battery)
                                    ctx.lineTo(point.x + 0.01, point.y);
                            }
                            previous = point;
                            latest = point;
                        }
                        ctx.stroke();
                        if (latest && latest.time === root.now) {
                            ctx.beginPath();
                            ctx.arc(latest.x, latest.y, 2.6, 0, Math.PI * 2);
                            ctx.fill();
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Repeater {
                model: [root.oldestLabel, root.battery ? "– – 50%" : "– – 100% load", "Now"]
                delegate: MonitorLabel {
                    required property int index
                    required property string modelData
                    Layout.fillWidth: true
                    Layout.horizontalStretchFactor: 1
                    horizontalAlignment: index === 0 ? Text.AlignLeft : index === 1 ? Text.AlignHCenter : Text.AlignRight
                    font.pixelSize: root.smallTextSize
                    opacity: 0.75
                    text: modelData
                }
            }
        }
    }
}
