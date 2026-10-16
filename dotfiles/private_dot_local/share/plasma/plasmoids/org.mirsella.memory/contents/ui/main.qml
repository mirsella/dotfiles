import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents
import org.kde.plasma.plasmoid
import org.kde.plasma.plasma5support as Plasma5Support

PlasmoidItem {
    id: root
    preferredRepresentation: fullRepresentation
    property var counters: null
    property var history: []
    property string failure: ""
    property bool running: false
    readonly property string command: "$HOME/.local/bin/plasma-memory-summary"
    readonly property int sampleCount: 150
    readonly property color ramColor: "#3daee9"
    readonly property color diskColor: "#e9783d"
    readonly property color zramColor: "#3de961"
    readonly property var series: [
        {key: "ramUsed", name: "RAM", color: ramColor},
        {key: "zramRam", name: "zram", color: zramColor},
        {key: "diskUsed", name: "Swap", color: diskColor}
    ]

    function gib(bytes, withUnit = true) {
        return (bytes / 1073741824).toLocaleString(Qt.locale(), "f", 1) + (withUnit ? " GiB" : "");
    }

    function poll() {
        if (!running) {
            running = true;
            executable.connectSource(command);
        }
    }

    Plasma5Support.DataSource {
        id: executable
        engine: "executable"
        connectedSources: []
        onNewData: (source, data) => {
            disconnectSource(source);
            root.running = false;
            try {
                if (data["exit code"] !== 0 || data["exit status"] !== 0) {
                    throw new Error(data["stderr"] || "Memory helper failed");
                }
                const next = JSON.parse(data["stdout"]);
                for (const key of ["ramUsed", "ramTotal", "ramAvailable", "diskUsed", "diskTotal", "zramData", "zramRam"]) {
                    if (!Number.isFinite(next[key]) || next[key] < 0) {
                        throw new Error("Invalid memory counter: " + key);
                    }
                }
                if (next.ramTotal === 0) {
                    throw new Error("Physical RAM capacity is zero");
                }
                if (next.zramRam > next.ramUsed) {
                    throw new Error("zram footprint exceeds physical RAM used");
                }
                root.counters = next;
                root.failure = "";
                const samples = root.history.slice(-(root.sampleCount - 1));
                samples.push(next);
                root.history = samples;
            } catch (error) {
                root.failure = "Memory counters unavailable";
                console.error(error);
                // A gap is more honest than extending stale values through a failure.
                const samples = root.history.slice(-(root.sampleCount - 1));
                samples.push(null);
                root.history = samples;
            }
        }
    }

    Timer {
        interval: 2000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.poll()
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: 500
        Layout.minimumHeight: 260
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Heading {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: "Memory"
            level: 2
        }

        Item {
            id: plot
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 100
            readonly property real maximum: root.counters ? Math.max(root.counters.ramTotal, root.counters.diskTotal) : 1

            Repeater {
                model: 5
                delegate: PlasmaComponents.Label {
                    required property int index
                    x: 0
                    y: (plot.height - height) * index / 4
                    text: root.counters ? (index === 4 ? "0 GiB" : root.gib(plot.maximum * (4 - index) / 4)) : "—"
                    font: Kirigami.Theme.smallFont
                    opacity: 0.75
                }
            }

            Canvas {
                id: chart
                anchors.fill: parent
                anchors.leftMargin: 66
                anchors.topMargin: 7
                anchors.bottomMargin: 7
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                Connections {
                    target: root
                    function onHistoryChanged() { chart.requestPaint(); }
                }
                Connections {
                    target: Kirigami.Theme
                    function onTextColorChanged() { chart.requestPaint(); }
                }
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.clearRect(0, 0, width, height);

                    function xAt(index) {
                        return width * (root.sampleCount - root.history.length + index) / (root.sampleCount - 1);
                    }
                    function yAt(index, key) {
                        const value = key ? root.history[index][key] : 0;
                        return height * (1 - Math.min(1, value / plot.maximum));
                    }
                    function fillBand(topKey, bottomKey, color, opacity) {
                        ctx.fillStyle = Qt.alpha(color, opacity);
                        let start = 0;
                        while (start < root.history.length) {
                            if (root.history[start] === null) {
                                start++;
                                continue;
                            }
                            let end = start + 1;
                            while (end < root.history.length && root.history[end] !== null) end++;
                            ctx.beginPath();
                            ctx.moveTo(xAt(start), yAt(start, topKey));
                            for (let i = start + 1; i < end; i++) ctx.lineTo(xAt(i), yAt(i, topKey));
                            for (let i = end - 1; i >= start; i--) ctx.lineTo(xAt(i), yAt(i, bottomKey));
                            ctx.closePath();
                            ctx.fill();
                            start = end;
                        }
                    }

                    // The blue band is only the remainder above zram, not RAM + zram.
                    fillBand("zramRam", null, root.zramColor, 0.4);
                    fillBand("ramUsed", "zramRam", root.ramColor, 0.4);
                    // Disk swap overlays from zero; it never increases the RAM stack.
                    fillBand("diskUsed", null, root.diskColor, 0.12);

                    ctx.strokeStyle = Qt.alpha(Kirigami.Theme.textColor, 0.18);
                    ctx.lineWidth = 1;
                    for (let i = 0; i <= 4; i++) {
                        const y = Math.min(height - 0.5, Math.max(0.5, i * height / 4));
                        ctx.beginPath();
                        ctx.moveTo(0, y);
                        ctx.lineTo(width, y);
                        ctx.stroke();
                    }
                    for (const series of root.series) {
                        ctx.strokeStyle = series.color;
                        ctx.lineWidth = 1.6;
                        ctx.setLineDash(series.key === "diskUsed" ? [6, 3] : []);
                        ctx.beginPath();
                        let connected = false;
                        for (let i = 0; i < root.history.length; i++) {
                            const sample = root.history[i];
                            if (sample === null) {
                                connected = false;
                                continue;
                            }
                            const x = xAt(i);
                            const y = yAt(i, series.key);
                            if (connected) ctx.lineTo(x, y);
                            else ctx.moveTo(x, y);
                            connected = true;
                        }
                        ctx.stroke();
                    }
                    ctx.setLineDash([]);
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing * 2
                Repeater {
                    model: root.series
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 8
                        Rectangle {
                            Layout.preferredWidth: 4
                            Layout.preferredHeight: 16
                            color: modelData.color
                        }
                        PlasmaComponents.Label {
                            text: modelData.name
                            color: modelData.color
                        }
                        PlasmaComponents.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: {
                                if (!root.counters || root.failure) return "—";
                                const c = root.counters;
                                if (modelData.key === "zramRam")
                                    return root.gib(c.zramData) + " in " + root.gib(c.zramRam);
                                const total = modelData.key === "diskUsed" ? c.diskTotal : c.ramTotal;
                                return root.gib(c[modelData.key], false) + " / " + root.gib(total);
                            }
                        }
                    }
                }
            }
            PlasmaComponents.Label {
                Layout.fillWidth: true
                visible: root.failure.length > 0
                text: root.failure
                color: Kirigami.Theme.negativeTextColor
            }
        }
    }
}
