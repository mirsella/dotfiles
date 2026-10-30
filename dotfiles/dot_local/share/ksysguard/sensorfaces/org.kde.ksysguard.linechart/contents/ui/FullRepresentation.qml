/*
    SPDX-FileCopyrightText: 2019 Marco Martin <mart@kde.org>
    SPDX-FileCopyrightText: 2019 David Edmundson <davidedmundson@kde.org>
    SPDX-FileCopyrightText: 2019 Arjen Hiemstra <ahiemstra@heimr.nl>
    SPDX-FileCopyrightText: 2020 David Redondo <kde@david-redondo.de>

    SPDX-License-Identifier: LGPL-2.0-or-later
*/

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import QtCore

import org.kde.kirigami as Kirigami

import org.kde.plasma.plasma5support as Plasma5Support

import org.kde.ksysguard.sensors as Sensors
import org.kde.ksysguard.faces as Faces
import org.kde.ksysguard.formatter as Formatter

import org.kde.quickcharts as Charts
import org.kde.quickcharts.controls as ChartsControls

Faces.SensorFace {
    id: root
    readonly property bool showLegend: controller.faceConfiguration.showLegend
    readonly property bool showGridLines: root.controller.faceConfiguration.showGridLines
    readonly property bool showYAxisLabels: root.controller.faceConfiguration.showYAxisLabels
    // Sensors that appear in both the high and low priority lists are drawn
    // again on top of the chart with a thicker line ("emphasized").
    readonly property var emphasizedSensorIds: root.controller.highPrioritySensorIds.filter(function(id) {
        return root.controller.lowPrioritySensorIds.indexOf(id) !== -1
    })
    // Map the hovered sensor into each chart's own list. An empty overlay
    // or a sensor absent from that chart must not receive a highlight index.
    function highlightedLineIndex(sensorIds) {
        if (legend.highlightedIndex < 0) {
            return -1
        }
        const legendSensorIds = legend.lowPriorityLegend
            ? root.controller.lowPrioritySensorIds
            : root.controller.highPrioritySensorIds
        return sensorIds.indexOf(legendSensorIds[legend.highlightedIndex])
    }

    // The CPU widget is the only one whose sensors start with "cpu/". Its
    // package power comes from the APU's SMU sensor; the total is the battery
    // discharge power, so it only exists while unplugged. A machine without a
    // battery reports no values and hides the readout.
    readonly property bool showPowerReadout: {
        for (var i = 0; i < root.controller.highPrioritySensorIds.length; ++i) {
            if (root.controller.highPrioritySensorIds[i].startsWith("cpu/")) {
                return true
            }
        }
        return false
    }

    // Disk usage charts use a single filesystem sensor. Draw 24h, 12h, 1h
    // and 1min histories together and replace the regular legend with a
    // compact timeframe key plus the current used fraction.
    readonly property string diskUsageSensorId: {
        for (var i = 0; i < root.controller.highPrioritySensorIds.length; ++i) {
            var id = root.controller.highPrioritySensorIds[i]
            if (id.startsWith("disk/") && id.endsWith("/usedPercent")) {
                return id
            }
        }
        return ""
    }
    readonly property bool showDiskTimeframes: diskUsageSensorId.length > 0
    readonly property string diskUsedSensorId: diskUsageSensorId.replace(/\/usedPercent$/, "/used")
    readonly property string diskTotalSensorId: diskUsageSensorId.replace(/\/usedPercent$/, "/total")
    readonly property string diskSummary: {
        var used = diskUsedSensor.value / 1e9
        var total = diskTotalSensor.value / 1e9
        if (!Number.isFinite(used) || !Number.isFinite(total) || total <= 0) {
            return ""
        }
        return used.toFixed(0) + "/" + total.toFixed(0) + " GB · " + (total - used).toFixed(0) + " GB free"
    }
    property real packageWatts: NaN
    property real totalWatts: NaN
    property string powerFailure: ""
    property bool powerRunning: false
    property bool powerUnsupported: false
    property var diskSamples: []
    property var diskRecentSamples: []
    readonly property int diskSampleInterval: 60000
    readonly property int diskRecentInterval: 5000
    readonly property int diskRecentCapacity: 13
    readonly property color diskColor24: {
        const configured = root.controller.sensorColors[root.diskUsageSensorId]
        return configured !== undefined && configured !== null ? configured : "#3daee9"
    }
    readonly property color diskColor12: "#2ecc71"
    readonly property color diskColor1: "#f67400"
    readonly property color diskColor1m: "#da4453"

    onShowDiskTimeframesChanged: {
        if (showDiskTimeframes) {
            restoreDiskSamples()
            restoreRecentSamples()
            sampleRecent()
            if (!sampleDisk()) {
                saveDiskHistory()
            }
        }
    }

    function parseSamples(serialized, windowMs) {
        const stored = JSON.parse(serialized)
        if (!Array.isArray(stored)) {
            throw new Error("history is not an array")
        }
        const now = Date.now()
        const samples = []
        for (const entry of stored) {
            if (!Array.isArray(entry) || entry.length < 2) {
                continue
            }
            const time = Number(entry[0])
            const value = Number(entry[1])
            if (Number.isFinite(time) && time <= now && time >= now - windowMs
                    && Number.isFinite(value) && value >= 0 && value <= 100) {
                samples.push({time: time, value: value})
            }
        }
        return samples.sort((a, b) => a.time - b.time)
    }

    function restoreDiskSamples() {
        if (diskHistorySettings.samplesJson.length === 0) {
            return
        }
        try {
            root.diskSamples = parseSamples(diskHistorySettings.samplesJson, 86400000).slice(-1440)
            diskCanvas.requestPaint()
        } catch (error) {
            console.warn("Ignoring invalid disk history:", error)
            diskHistorySettings.samplesJson = ""
            diskHistorySettings.sync()
        }
    }

    function restoreRecentSamples() {
        if (diskHistorySettings.recentJson.length === 0) {
            return
        }
        try {
            root.diskRecentSamples = parseSamples(diskHistorySettings.recentJson, 60000)
                .slice(-root.diskRecentCapacity)
            diskCanvas.requestPaint()
        } catch (error) {
            console.warn("Ignoring invalid recent disk history:", error)
            diskHistorySettings.recentJson = ""
            diskHistorySettings.sync()
        }
    }

    function saveDiskHistory() {
        diskHistorySettings.samplesJson = JSON.stringify(root.diskSamples.map(sample => [sample.time, sample.value]))
        diskHistorySettings.recentJson = JSON.stringify(root.diskRecentSamples.map(sample => [sample.time, sample.value]))
        diskHistorySettings.sync()
    }

    function currentDiskPercent() {
        const percent = diskPercentSensor.value
        if (Number.isFinite(percent) && percent >= 0 && percent <= 100) {
            return percent
        }
        const used = diskUsedSensor.value
        const total = diskTotalSensor.value
        if (Number.isFinite(used) && Number.isFinite(total) && total > 0) {
            return used * 100 / total
        }
        return NaN
    }

    function sampleDisk() {
        const value = currentDiskPercent()
        if (!Number.isFinite(value)) {
            return false
        }
        const now = Date.now()
        const samples = root.diskSamples.filter(sample => now - sample.time <= 86400000)
        if (samples.length > 0 && now - samples[samples.length - 1].time < root.diskSampleInterval) {
            root.diskSamples = samples
            return false
        }
        samples.push({time: now, value: value})
        if (samples.length > 1440) {
            samples.splice(0, samples.length - 1440)
        }
        root.diskSamples = samples
        saveDiskHistory()
        diskCanvas.requestPaint()
        return true
    }

    function sampleRecent() {
        const value = currentDiskPercent()
        if (!Number.isFinite(value)) {
            return
        }
        const now = Date.now()
        const samples = root.diskRecentSamples.filter(sample => now - sample.time <= 60000)
        if (samples.length > 0 && now - samples[samples.length - 1].time < root.diskRecentInterval) {
            root.diskRecentSamples = samples
            return
        }
        samples.push({time: now, value: value})
        if (samples.length > root.diskRecentCapacity) {
            samples.splice(0, samples.length - root.diskRecentCapacity)
        }
        root.diskRecentSamples = samples
        diskCanvas.requestPaint()
    }

    function watts(value) {
        return value.toLocaleString(Qt.locale(), "f", 0) + " W"
    }

    Settings {
        id: diskHistorySettings
        category: "history"
        // Six levels up from the installed face directory is ~/.local, where
        // this state file stays out of the managed configuration tree.
        location: Qt.resolvedUrl("../../../../../../state/ksysguard-disk-history.conf")
        property string samplesJson: ""
        property string recentJson: ""
    }

    Component.onDestruction: {
        if (showDiskTimeframes) {
            saveDiskHistory()
        }
    }

    Timer {
        interval: root.diskSampleInterval
        repeat: true
        running: root.showDiskTimeframes
        onTriggered: {
            root.sampleRecent()
            if (!root.sampleDisk()) {
                root.saveDiskHistory()
            }
        }
    }

    Timer {
        interval: root.diskRecentInterval
        repeat: true
        running: root.showDiskTimeframes
        onTriggered: root.sampleRecent()
    }

    TextMetrics {
        id: wattValueMetrics
        font: legend.font
        text: "999 W"
    }

    Sensors.Sensor {
        id: diskUsedSensor
        sensorId: root.diskUsedSensorId
        enabled: root.showDiskTimeframes
        updateRateLimit: root.controller.updateRateLimit
        onValueChanged: {
            root.sampleDisk()
            root.sampleRecent()
        }
    }

    Sensors.Sensor {
        id: diskTotalSensor
        sensorId: root.diskTotalSensorId
        enabled: root.showDiskTimeframes
        updateRateLimit: root.controller.updateRateLimit
        onValueChanged: {
            root.sampleDisk()
            root.sampleRecent()
        }
    }

    Sensors.Sensor {
        id: diskPercentSensor
        sensorId: root.diskUsageSensorId
        enabled: root.showDiskTimeframes
        updateRateLimit: root.controller.updateRateLimit
        onValueChanged: {
            root.sampleDisk()
            root.sampleRecent()
        }
    }

    Plasma5Support.DataSource {
        id: powerExecutable
        engine: "executable"
        onNewData: (source, data) => {
            disconnectSource(source)
            root.powerRunning = false
            try {
                if (data["exit code"] !== 0 || data["exit status"] !== 0) {
                    throw new Error(data["stderr"] || "Power helper failed")
                }
                const next = JSON.parse(data["stdout"])
                for (const key of ["packageWatts", "totalWatts"]) {
                    if (next[key] !== null && (!Number.isFinite(next[key]) || next[key] < 0 || next[key] > 1000)) {
                        throw new Error("Invalid power value: " + key)
                    }
                }
                if (next.packageWatts === null && next.totalWatts === null) {
                    // No battery: this machine has no laptop power to report.
                    // Stop polling until the widget or helper changes.
                    root.powerUnsupported = true
                    root.packageWatts = NaN
                    root.totalWatts = NaN
                    root.powerFailure = ""
                    return
                }
                root.packageWatts = Number.isFinite(next.packageWatts) ? next.packageWatts : NaN
                root.totalWatts = Number.isFinite(next.totalWatts) ? next.totalWatts : NaN
                root.powerFailure = ""
            } catch (error) {
                root.packageWatts = NaN
                root.totalWatts = NaN
                root.powerFailure = String(error)
                console.error(error)
            }
        }
    }

    Timer {
        interval: 2000
        running: root.showPowerReadout && !root.powerUnsupported
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            if (!root.powerRunning) {
                root.powerRunning = true
                powerExecutable.connectSource("$HOME/.local/bin/plasma-power-summary")
            }
        }
    }

    // Keeps a chart's value sources in sync with a sensor data model: one
    // history source per sensor, sampled like the main chart. Used for the
    // emphasized overlay and the hidden legend chart.
    component SensorHistorySources: Instantiator {
        required property Charts.Chart chart
        required property Sensors.SensorDataModel sensorData
        property int historySeconds: root.controller.faceConfiguration.historyAmount

        model: sensorData.sensors

        delegate: Charts.HistoryProxySource {
            id: history

            source: Charts.ModelSource {
                model: sensorData
                column: index
                roleName: "Value"
            }
            interval: {
                if (root.controller.updateRateLimit > 0) {
                    return root.controller.updateRateLimit
                }
                if (sensorData.ready) {
                    return sensorData.headerData(index, Qt.Horizontal, Sensors.SensorDataModel.UpdateInterval) ?? 0
                }
                return 0
            }
            maximumHistory: interval > 0 ? (historySeconds * 1000) / interval : 0
            fillMode: Charts.HistoryProxySource.FillFromEnd
            property var connection: Connections {
                target: root.controller
                function onUpdateRateLimitChanged() {
                    history.clear()
                }
            }
        }

        onObjectAdded: (index, object) => chart.insertValueSource(index, object)
        onObjectRemoved: (object) => chart.removeValueSource(object)
    }

    // Arbitrary minimumWidth to make easier to align plasmoids in a predictable way
    Layout.minimumWidth: Kirigami.Units.gridUnit * 8
    Layout.preferredWidth: Math.max(titleMetrics.width, legend.preferredWidth)

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Heading {
            id: heading
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            text: root.controller.title
            visible: root.controller.showTitle && text.length > 0
            level: 2
            TextMetrics {
                id: titleMetrics
                font: heading.font
                text: heading.text
            }
        }

        RowLayout {
            spacing: Kirigami.Units.smallSpacing
            Layout.fillHeight: true
            Layout.topMargin: showYAxisLabels && !root.showDiskTimeframes ? axisMetrics.height / 2 : 0
            Layout.bottomMargin: Layout.topMargin

            Layout.minimumHeight: 3 * Kirigami.Units.gridUnit
            Layout.preferredHeight: 5 * Kirigami.Units.gridUnit

            ChartsControls.AxisLabels {
                id: axisLabels
                visible: showYAxisLabels && !root.showDiskTimeframes
                Layout.fillHeight: true
                constrainToBounds: false
                direction: ChartsControls.AxisLabels.VerticalBottomTop
                delegate: QQC2.Label {
                    anchors.right: parent.right
                    font: Kirigami.Theme.smallFont
                    text: Formatter.Formatter.formatValueShowNull(ChartsControls.AxisLabels.label,
                        compactRepresentation.sensorsModel.unit)
                    opacity: 0.75
                }
                source: Charts.ChartAxisSource {
                    chart: compactRepresentation
                    axis: Charts.ChartAxisSource.YAxis
                    itemCount: 5
                }
                TextMetrics {
                    id: axisMetrics
                    font: Kirigami.Theme.smallFont
                    text: Formatter.Formatter.formatValueShowNull("0",
                        compactRepresentation.sensorsModel.unit)
                }
            }
            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                Item {
                    id: mainChartViewport
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.right: parent.right
                    width: parent.width
                    visible: !root.showDiskTimeframes
                    enabled: visible
                    clip: true

                    Item {
                        id: mainChartArea
                        width: mainChartViewport.parent ? mainChartViewport.parent.width : mainChartViewport.width
                        height: mainChartViewport.height
                        anchors.right: parent.right

                        LineChart {
                            id: compactRepresentation
                            anchors.fill: parent

                            controller: root.controller

                            highlight: root.highlightedLineIndex(root.controller.highPrioritySensorIds)

                            ChartsControls.GridLines {
                                id: horizontalLines
                                visible: showGridLines
                                direction: ChartsControls.GridLines.Vertical
                                anchors.fill: compactRepresentation
                                z: compactRepresentation.z - 1
                                chart: compactRepresentation

                                major.count: 3
                                major.lineWidth: 1
                                // The same color as a Kirigami.Separator
                                major.color: Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
                                minor.visible: false
                            }
                        }

                        // Emphasized overlay: same geometry and Y range as the
                        // main chart, but only the emphasized sensors are drawn
                        // and with a thicker line so they stand out.
                        Charts.LineChart {
                            id: emphasizedChart
                            anchors.fill: parent
                            visible: !root.showDiskTimeframes && root.emphasizedSensorIds.length > 0
                            z: 1

                            highlight: root.highlightedLineIndex(root.emphasizedSensorIds)

                            direction: compactRepresentation.direction
                            interpolate: compactRepresentation.interpolate
                            stacked: false
                            fillOpacity: 0
                            lineWidth: 2

                            yRange.automatic: false
                            yRange.from: compactRepresentation.yRange.from
                            yRange.to: compactRepresentation.yRange.to

                            colorSource: Charts.MapProxySource {
                                source: Charts.ArraySource {
                                    array: root.emphasizedSensorIds
                                }
                                map: root.controller.sensorColors
                            }

                            Sensors.SensorDataModel {
                                id: emphasizedSensorsModel
                                sensors: root.emphasizedSensorIds
                                updateRateLimit: root.controller.updateRateLimit
                            }

                            SensorHistorySources {
                                chart: emphasizedChart
                                sensorData: emphasizedSensorsModel
                            }
                        }
                    }
                }

                Canvas {
                    id: diskCanvas
                    anchors.fill: parent
                    visible: root.showDiskTimeframes
                    renderTarget: Canvas.FramebufferObject

                    onVisibleChanged: {
                        if (visible) {
                            requestPaint()
                        }
                    }
                    onWidthChanged: requestPaint()
                    onHeightChanged: requestPaint()

                    onPaint: {
                        const ctx = getContext("2d")
                        const left = 28
                        const right = 6
                        const top = 6
                        const bottom = 16
                        const plotWidth = Math.max(1, width - left - right)
                        const plotHeight = Math.max(1, height - top - bottom)

                        ctx.clearRect(0, 0, width, height)
                        ctx.font = "10px sans-serif"
                        ctx.textAlign = "right"
                        ctx.textBaseline = "middle"
                        ctx.lineWidth = 1

                        for (let percent = 0; percent <= 100; percent += 25) {
                            const y = top + plotHeight * (1 - percent / 100)
                            ctx.beginPath()
                            ctx.globalAlpha = 0.2
                            ctx.strokeStyle = String(Kirigami.Theme.textColor)
                            ctx.moveTo(left, y)
                            ctx.lineTo(width - right, y)
                            ctx.stroke()
                            ctx.globalAlpha = 0.75
                            ctx.fillStyle = String(Kirigami.Theme.disabledTextColor)
                            ctx.fillText(percent + "%", left - 4, y)
                        }

                        const paintSeries = function(samples, windowMs, color, sampleInterval) {
                            const now = Date.now()
                            const visibleSamples = samples.filter(sample => sample.time >= now - windowMs
                                && sample.time <= now && Number.isFinite(sample.value))
                            if (visibleSamples.length === 0) {
                                return
                            }

                            const strokeColor = String(color)
                            const xAt = time => left + plotWidth * (1 - (now - time) / windowMs)
                            const yAt = value => top + plotHeight * (1 - Math.max(0, Math.min(100, value)) / 100)
                            let segment = []

                            const flushSegment = function() {
                                if (segment.length === 1) {
                                    ctx.beginPath()
                                    ctx.arc(xAt(segment[0].time), yAt(segment[0].value), 2, 0, 2 * Math.PI)
                                    ctx.fillStyle = strokeColor
                                    ctx.fill()
                                } else if (segment.length > 1) {
                                    ctx.beginPath()
                                    for (let i = 0; i < segment.length; ++i) {
                                        const x = xAt(segment[i].time)
                                        const y = yAt(segment[i].value)
                                        if (i === 0) {
                                            ctx.moveTo(x, y)
                                        } else {
                                            ctx.lineTo(x, y)
                                        }
                                    }
                                    ctx.strokeStyle = strokeColor
                                    ctx.lineWidth = 2
                                    ctx.lineJoin = "round"
                                    ctx.lineCap = "round"
                                    ctx.stroke()
                                }
                                segment = []
                            }

                            ctx.globalAlpha = 1
                            for (const sample of visibleSamples) {
                                if (segment.length > 0
                                        && sample.time - segment[segment.length - 1].time > sampleInterval * 2) {
                                    flushSegment()
                                }
                                segment.push(sample)
                            }
                            flushSegment()
                        }

                        paintSeries(root.diskSamples, 86400000, root.diskColor24, root.diskSampleInterval)
                        paintSeries(root.diskSamples, 43200000, root.diskColor12, root.diskSampleInterval)
                        paintSeries(root.diskSamples, 3600000, root.diskColor1, root.diskSampleInterval)
                        paintSeries(root.diskRecentSamples, 60000, root.diskColor1m, root.diskRecentInterval)
                    }
                }

                QQC2.Label {
                    anchors {
                        right: parent.right
                        bottom: parent.bottom
                    }
                    visible: root.showDiskTimeframes
                    z: 4
                    text: "now"
                    font: Kirigami.Theme.smallFont
                    opacity: 0.7
                }
            }
        }

        // The legend lists the lines of the chart, as usual for widgets using
        // this face. A widget that additionally configures low priority
        // sensors (like the CPU temperature) gets a short legend limited to
        // those sensors instead of every high priority line; the rows are
        // read from a hidden chart carrying only those sensors so they keep
        // their names, colors and live values.
        Faces.ExtendedLegend {
            id: legend
            Layout.fillWidth: true
            Layout.minimumHeight: implicitHeight
            visible: root.showLegend && !root.showDiskTimeframes
            horizontalSpacing: root.showPowerReadout ? Kirigami.Units.smallSpacing : Kirigami.Units.gridUnit
            readonly property bool lowPriorityLegend: root.controller.lowPrioritySensorIds.length > 0
            chart: legend.lowPriorityLegend ? legendChart : compactRepresentation
            sourceModel: root.showLegend ? (legend.lowPriorityLegend ? legendSensorsModel : compactRepresentation.sensorsModel) : null

            ChartsControls.LegendDelegate {
                id: packageLegend
                visible: root.showPowerReadout && Number.isFinite(root.packageWatts)
                name: "CPU"
                value: root.watts(root.packageWatts)
                indicator: null
                maximumValueWidth: wattValueMetrics.width
                ChartsControls.LegendLayout.minimumWidth: minimumWidth
                ChartsControls.LegendLayout.preferredWidth: preferredWidth
                ChartsControls.LegendLayout.maximumWidth: Math.max(legend.maximumDelegateWidth, preferredWidth)

                HoverHandler {
                    id: packageHover
                }
                QQC2.ToolTip.visible: packageHover.hovered
                QQC2.ToolTip.text: "APU package power (CPU and integrated GPU)"
            }

            ChartsControls.LegendDelegate {
                id: totalLegend
                visible: root.showPowerReadout && Number.isFinite(root.totalWatts)
                name: "Total"
                value: root.watts(root.totalWatts)
                indicator: null
                maximumValueWidth: wattValueMetrics.width
                ChartsControls.LegendLayout.minimumWidth: minimumWidth
                ChartsControls.LegendLayout.preferredWidth: preferredWidth
                ChartsControls.LegendLayout.maximumWidth: Math.max(legend.maximumDelegateWidth, preferredWidth)

                HoverHandler {
                    id: totalHover
                }
                QQC2.ToolTip.visible: totalHover.hovered
                QQC2.ToolTip.text: "Whole-laptop draw, measured from battery voltage and current"
            }

            ChartsControls.LegendDelegate {
                visible: root.showPowerReadout && root.powerFailure.length > 0
                name: "Power"
                value: "n/a"
                color: Kirigami.Theme.negativeTextColor
                ChartsControls.LegendLayout.minimumWidth: minimumWidth
                ChartsControls.LegendLayout.preferredWidth: preferredWidth
                ChartsControls.LegendLayout.maximumWidth: Math.max(legend.maximumDelegateWidth, preferredWidth)

                HoverHandler {
                    id: powerFailureHover
                }
                QQC2.ToolTip.visible: powerFailureHover.hovered
                QQC2.ToolTip.text: root.powerFailure
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit + Kirigami.Units.smallSpacing
            Layout.maximumHeight: Layout.preferredHeight
            visible: root.showDiskTimeframes && root.showLegend
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                spacing: Kirigami.Units.smallSpacing

                Repeater {
                    model: [
                        { label: "1min", color: root.diskColor1m },
                        { label: "1h", color: root.diskColor1 },
                        { label: "12h", color: root.diskColor12 },
                        { label: "24h", color: root.diskColor24 }
                    ]

                    delegate: RowLayout {
                        spacing: 2

                        Rectangle {
                            Layout.alignment: Qt.AlignVCenter
                            implicitWidth: 6
                            implicitHeight: 6
                            radius: 3
                            color: modelData.color
                        }
                        QQC2.Label {
                            text: modelData.label
                            font: Kirigami.Theme.smallFont
                        }
                    }
                }
            }

            QQC2.Label {
                Layout.fillWidth: true
                text: root.diskSummary
                font: Kirigami.Theme.defaultFont
                horizontalAlignment: Text.AlignRight
                elide: Text.ElideRight
            }
        }

        // Hidden helper chart: draws nothing, but holds one value source per
        // low priority sensor so the legend can read their names, colors and
        // live values the same way it does for regular chart lines.
        Charts.LineChart {
            id: legendChart
            visible: false

            // Sensors without a configured color would get a black chip in
            // the legend, which looks broken; map them to a transparent
            // color so only entries with a real chart color (the
            // temperature) show an indicator.
            colorSource: Charts.MapProxySource {
                source: Charts.ArraySource {
                    array: root.controller.lowPrioritySensorIds
                }
                map: {
                    var colors = {};
                    for (var i = 0; i < root.controller.lowPrioritySensorIds.length; ++i) {
                        var id = root.controller.lowPrioritySensorIds[i];
                        colors[id] = root.controller.sensorColors[id] || "transparent";
                    }
                    return colors;
                }
            }

            nameSource: Charts.ModelSource {
                model: legendSensorsModel
                roleName: "Name"
                indexColumns: true
            }
            shortNameSource: Charts.ModelSource {
                model: legendSensorsModel
                roleName: "ShortName"
                indexColumns: true
            }

            SensorHistorySources {
                chart: legendChart
                sensorData: legendSensorsModel
            }
        }

        Sensors.SensorDataModel {
            id: legendSensorsModel
            // Feeds the hidden legend chart and the legend's unit lookups.
            sensors: root.controller.lowPrioritySensorIds
            updateRateLimit: root.controller.updateRateLimit
            // The default CPU sensor names are long enough to crowd the
            // one-line legend and elide the values next to them. Keep labels
            // configured for this widget, if any.
            sensorLabels: {
                if (!root.showPowerReadout) {
                    return root.controller.sensorLabels
                }
                const labels = Object.assign({}, root.controller.sensorLabels)
                if (!labels["cpu/all/averageFrequency"]) {
                    labels["cpu/all/averageFrequency"] = "Avg CPU Freq"
                }
                if (!labels["cpu/all/averageTemperature"]) {
                    labels["cpu/all/averageTemperature"] = "Avg Temp"
                }
                return labels
            }
        }
    }
}
