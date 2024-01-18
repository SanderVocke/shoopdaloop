import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

Item {
    id: root
    
    property var loop // LoopWidget
    property var composite_loop : loop.maybe_composite_loop

    property int cycle_width: 50
    property int track_height: 26

    property var schedule: composite_loop.schedule

    property var sync_track
    property var main_tracks : []

    Column {
        spacing: 1

        Mapper {
            model : [sync_track].concat(main_tracks)

            Track {
                property var mapped_item
                property int index

                track : mapped_item

                height: root.track_height
                width: root.width
            }
        }
    }

    component Track : Item {
        property var track

        readonly property int track_idx : track.track_idx
        readonly property var track_schedule : {
            if (!root.schedule) { return {}; }
            var rval = root.schedule
            for(var k of Array.from(Object.keys(rval))) { //iteration
                for(var kk of Array.from(Object.keys(rval[k]))) { //loops start / loops end for this iteration
                    rval[k][kk] = rval[k][kk].filter(function (x) { return x.track_idx == track_idx; })
                }
            }
            return rval
        }

        Label {
            id: name_label
            anchors {
                left: parent.left
                verticalCenter: parent.verticalCenter
            }
            text: track.name
            horizontalAlignment: Text.AlignRight
            width: 80
        }

        Rectangle {
            anchors {
                top: parent.top
                bottom: parent.bottom
                left: name_label.right
                right: parent.right
                leftMargin: 5
            }

            border.color: 'black'
            border.width: 1
            color: 'transparent'
        }
    }
}