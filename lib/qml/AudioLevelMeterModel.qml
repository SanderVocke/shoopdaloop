import QtQuick 2.15

QtObject {
    property real decrease_rate: 60.0
    property real max_dt: 0.1
    property real input: 0.0
    property real prev_time: 0.0
    property real value: -1000000.0

    onInputChanged: {
        const d = new Date();
        var time = d.getTime()
        var dt = Math.min(max_dt, (time - prev_time) / 1000.0)

        var db = 20.0 * Math.log(input) / Math.log(10.0)

        if (db > value) {
            value = db
        } else {
            value = value - decrease_rate * dt
        }

        prev_time = time
    }
}