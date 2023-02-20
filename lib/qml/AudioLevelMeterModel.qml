import QtQuick 2.15

Item {
    property real max_decrease_rate: 40.0
    property real max_decrease_rate_acceleration: 10000.0
    property real max_dt: 0.1
    property real input: 0.0
    property real prev_decrease_rate: 0.0
    property real prev_time: 0.0
    property real value: -1000000.0

    // In case the input doesn't change, we should
    // still update the output value
    Timer {
        interval: 30
        running: true
        repeat: true
        onTriggered: update()
    }

    onInputChanged: update()
    
    function update() {
        const d = new Date();
        var time = d.getTime()
        var dt = Math.min(max_dt, (time - prev_time) / 1000.0)

        var db = 20.0 * Math.log(input) / Math.log(10.0)

        if (db > value) {
            value = db
            prev_decrease_rate = 0.0
        } else {
            prev_decrease_rate = Math.min(prev_decrease_rate + max_decrease_rate_acceleration, max_decrease_rate)
            value = value - prev_decrease_rate * dt
        }

        prev_time = time
    }
}