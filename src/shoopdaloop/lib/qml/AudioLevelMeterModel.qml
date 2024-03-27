import QtQuick 6.6

Item {
    id: root
    property real max_decrease_rate: 40.0
    property real max_decrease_rate_acceleration: 10000.0
    property real max_dt: 0.1

    property real bottom_db: -200.0
    property real input: 0.0
    property real input_db: Math.max(bottom_db, 20.0 * Math.log(input) / Math.log(10.0))
    
    property real value: bottom_db

    Behavior on input_db {
        NumberAnimation {
            properties: "value"
            target: root
            duration: input_db > value ? 0 : 4000
            to: input_db > value ? input_db : bottom_db
        }
    }

    onInputChanged: update()
}