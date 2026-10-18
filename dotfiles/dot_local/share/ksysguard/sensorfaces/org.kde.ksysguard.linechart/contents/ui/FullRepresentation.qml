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

import org.kde.kirigami as Kirigami

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
    // Maps the highlighted legend row to the index of the line it points at:
    // the plain row index when the legend lists the chart's own lines, or
    // the sensor's index in the given list when the legend lists low
    // priority sensors instead.
    function highlightedLineIndex(sensorIds) {
        if (!legend.lowPriorityLegend || legend.highlightedIndex < 0) {
            return legend.highlightedIndex
        }
        return sensorIds.indexOf(root.controller.lowPrioritySensorIds[legend.highlightedIndex])
    }

    // Keeps a chart's value sources in sync with a sensor data model: one
    // history source per sensor, sampled like the main chart. Used for the
    // emphasized overlay and the hidden legend chart.
    component SensorHistorySources: Instantiator {
        required property Charts.Chart chart
        required property Sensors.SensorDataModel sensorData

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
            maximumHistory: interval > 0 ? (root.controller.faceConfiguration.historyAmount * 1000) / interval : 0
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
            Layout.topMargin: showYAxisLabels ? axisMetrics.height / 2 : 0
            Layout.bottomMargin: Layout.topMargin

            Layout.minimumHeight: 3 * Kirigami.Units.gridUnit
            Layout.preferredHeight: 5 * Kirigami.Units.gridUnit

            ChartsControls.AxisLabels {
                id: axisLabels
                visible: showYAxisLabels
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

                // Emphasized overlay: same geometry and Y range as the main
                // chart, but only the emphasized sensors are drawn and with a
                // thicker line so they stand out among the regular ones.
                Charts.LineChart {
                    id: emphasizedChart
                    anchors.fill: parent
                    visible: root.emphasizedSensorIds.length > 0
                    z: 1

                    highlight: root.highlightedLineIndex(root.emphasizedSensorIds)

                    direction: compactRepresentation.direction
                    interpolate: compactRepresentation.interpolate
                    stacked: false
                    fillOpacity: 0
                    lineWidth: 4

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
                        sensorLabels: root.controller.sensorLabels
                        sensorColors: root.controller.sensorColors
                    }

                    SensorHistorySources {
                        chart: emphasizedChart
                        sensorData: emphasizedSensorsModel
                    }
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
            visible: root.showLegend
            readonly property bool lowPriorityLegend: root.controller.lowPrioritySensorIds.length > 0
            chart: legend.lowPriorityLegend ? legendChart : compactRepresentation
            sourceModel: root.showLegend ? (legend.lowPriorityLegend ? legendSensorsModel : compactRepresentation.sensorsModel) : null
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
            sensorLabels: root.controller.sensorLabels
            sensorColors: root.controller.sensorColors
        }
    }
}
