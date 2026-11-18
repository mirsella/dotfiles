import QtTest
import "../../modules/home/plasmoids/keyboard-battery/BatteryTrend.js" as Trend

TestCase {
    name: "BatteryTrend"

    readonly property double day: 24 * Trend.hourMs
    readonly property double now: 30 * day

    function test_recordKeepsRecentReadings() {
        const samples = [
            { time: now - 15 * day, values: { central: 90 } },
            { time: now - 7 * day, values: { central: 86 } },
        ];
        const recorded = Trend.record(samples, now, { central: 80 });
        compare(recorded.length, 2);
        compare(recorded[0].values.central, 86);
        compare(recorded[1].values.central, 80);
        compare(Trend.record(recorded, now + 1000, { central: 79, peripheral: 44 }), [
            ...recorded,
            { time: now + 1000, values: { central: 79, peripheral: 44 } },
        ]);
        verify(Trend.record(recorded, now + 1000, {}) === recorded);
        verify(Trend.record(recorded, now + 1000, { central: 80 }) === recorded);
        verify(Trend.record(recorded, now + 1000, { central: 81 }) === recorded);
        compare(Trend.record(recorded, now + 1000, { central: 83 }).length, 3);
        compare(Trend.record(recorded, now + Trend.intervalMs, { central: 80 }).length, 3);
        compare(Trend.record(recorded, now + 15 * day, {}).length, 0);
    }

    function test_forecastsOnTheFirstDrop() {
        const start = now - Trend.intervalMs / 2;
        const samples = Trend.record([], start, { central: 80, peripheral: 50 });
        compare(Trend.estimate(samples, "central", now, 80), null);
        const recorded = Trend.record(samples, now, { central: 79, peripheral: 50 });
        compare(recorded.length, 2);
        compare(Trend.estimate(recorded, "central", now, 79), {
            lowHours: 9.8125,
            highHours: null,
        });
        compare(Trend.estimate(recorded, "peripheral", now, 50), null);
    }

    function test_moreDischargeTightensTheRange() {
        const samples = [{ time: now - day, values: { central: 80 } }];
        const early = Trend.estimate(samples, "central", now, 78);
        const later = Trend.estimate(samples, "central", now + 4 * day, 70);
        verify(early.highHours > early.lowHours);
        verify(later.highHours > later.lowHours);
        verify(later.highHours / later.lowHours < early.highHours / early.lowHours);
    }

    function test_estimatesEachHalfIndependently() {
        const samples = [
            { time: now - 4 * day, values: { central: 88, peripheral: 52 } },
            { time: now - 3 * day, values: { central: 86, peripheral: 52 } },
            { time: now - 2 * day, values: { central: 84, peripheral: 51 } },
            { time: now - day, values: { central: 82, peripheral: 51 } },
        ];
        compare(Trend.estimate(samples, "central", now, 80), {
            lowHours: 848,
            highHours: 1104,
        });
        compare(Trend.estimate(samples, "peripheral", now, 51), {
            lowHours: 2424,
            highHours: null,
        });
        compare(Trend.estimate(samples, "central", now, null), null);
        compare(Trend.estimate(samples, "central", now, 0), null);
        compare(Trend.estimate(samples, "central", now, 90), null);
    }

    function test_rechargeDiscardsEarlierTrend() {
        const samples = [
            { time: now - 4 * day, values: { central: 85, peripheral: 60 } },
            { time: now - 3 * day, values: { central: 80, peripheral: 58 } },
            { time: now - 2 * day, values: { central: 75, peripheral: 56 } },
            { time: now - day, values: { central: 95, peripheral: 54 } },
            { time: now - Trend.hourMs, values: { central: 90 } },
        ];
        compare(Trend.estimate(samples, "central", now, 89), {
            lowHours: 88.5 * 24 / 7,
            highHours: 89.5 * 24 / 5,
        });
        compare(Trend.estimate(samples, "peripheral", now, 52), {
            lowHours: 51.5 * 96 / 9,
            highHours: 720,
        });
    }
}
