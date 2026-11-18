.pragma library

const hourMs = 60 * 60 * 1000;
const windowMs = 14 * 24 * hourMs;
const intervalMs = hourMs / 2;
const gapMs = 3 * intervalMs;
const rechargePoints = 3;

function record(samples, now, values) {
    const retained = samples.filter(sample => sample.time >= now - windowMs && sample.time <= now);
    const last = retained[retained.length - 1];
    const keys = Object.keys(values);
    // Besides the chart's regular samples, keep new lows and likely
    // recharges so a quick change is not lost before the next chart sample.
    const changed = key => {
        for (let i = retained.length - 1; i >= 0; --i) {
            const previous = retained[i].values[key];
            if (previous != null)
                return values[key] < previous || values[key] >= previous + rechargePoints;
        }
        return true;
    };
    if (keys.length && (!last || now - last.time >= intervalMs || keys.some(changed))) {
        retained.push({ time: now, values: values });
        return retained;
    }
    return retained.length === samples.length ? samples : retained;
}

function estimate(samples, key, now, level) {
    if (level == null || level === 0)
        return null;

    let start = null;
    let low = null;
    for (const sample of samples) {
        const value = sample.values[key];
        if (value == null || sample.time < now - windowMs || sample.time > now)
            continue;
        // A rise of three points is treated as a recharge. Do not use the
        // prior discharge cycle; smaller increases may just be gauge noise.
        if (!start || value >= low + rechargePoints) {
            start = { time: sample.time, value: value };
            low = value;
        }
        low = Math.min(low, value);
    }
    if (!start || level >= low + rechargePoints)
        return null;

    const elapsedHours = (now - start.time) / hourMs;
    const drop = start.value - level;
    if (elapsedHours <= 0 || drop < 1)
        return null;

    // Both readings are whole-percent values. Their difference can be
    // off by one point. This range covers the gauge resolution, not future use.
    return {
        lowHours: (level - 0.5) * elapsedHours / (drop + 1),
        highHours: drop > 1 ? Math.min(level + 0.5, 100) * elapsedHours / (drop - 1) : null
    };
}
