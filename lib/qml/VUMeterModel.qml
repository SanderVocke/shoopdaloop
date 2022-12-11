import QtQuick 2.15

QtObject {
    property real charge_rate: 10000.0
    property real discharge_rate: 3.0
    property real max_dt: 0.1
    property real input: 0.0
    property real prev_input: 0.0
    property real prev_time: 0.0
    property real value: 0.0

    onInputChanged: {
        const d = new Date();
        var time = d.getTime()
        var dt = Math.min(max_dt, (time - prev_time) / 1000.0)

        if (input > value) {
            value = Math.min(
                value + (input-value) * charge_rate * dt,
                input
                )
        } else {
            value = value * (1 - discharge_rate * dt)
        }

        prev_input = input
        prev_time = time
    }
}