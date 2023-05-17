import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import FetchChannelData

import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    property var loop
    property var master_loop

    enum Tool {
        SetStartOffsetAll,
        SetStartOffsetSingle,
        SetLength
    }

    Row {
        id: toolbar_1
        anchors {
            top: parent.top
            left: parent.left
        }
        height: 40
        spacing: 5

        ExtendedButton {
            tooltip: "Re-fetch and render loop data."
            height: 35
            width: 30
            //onClicked: root.request_update_data()

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'refresh'
                color: Material.foreground
            }
        }

        Label {
            anchors.verticalCenter: zoom_slider.verticalCenter
            text: "Zoom:"
        }

        Slider {
            id: zoom_slider
            width: 150
            value: 8.0
            from: 16.0
            to: 0.0
            property real samples_per_pixel: Math.pow(2.0, value)
            anchors {
                verticalCenter: parent.verticalCenter
            }
        }

        Label {
            anchors.verticalCenter: zoom_slider.verticalCenter
            text: "Tool:"
        }

        ComboBox {
            id: tool_combo
            anchors.verticalCenter: zoom_slider.verticalCenter
            textRole: "text"
            valueRole: "value"
            width: 120

            model: [
                { value: LoopContentWidget.Tool.SetStartOffsetAll, text: "set start (all)" },
                { value: LoopContentWidget.Tool.SetStartOffsetSingle, text: "set start (single)" },
                { value: LoopContentWidget.Tool.SetLength, text: "set length" }
            ]
        }

        Label {
            anchors.verticalCenter: zoom_slider.verticalCenter
            text: "Grid:"
        }

        ComboBox {
            id: minor_grid_divider
            anchors.verticalCenter: zoom_slider.verticalCenter
            textRole: "text"
            valueRole: "value"
            currentIndex : 5
            width: 120

            model: [
                { value: undefined, text: "None" },
                { value: 2, text: "Master / 2" },
                { value: 3, text: "Master / 3" },
                { value: 4, text: "Master / 4" },
                { value: 6, text: "Master / 6" },
                { value: 8, text: "Master / 8" },
                { value: 16, text: "Master / 16" },
            ]
        }
    }

    Row {
        id: toolbar_2
        anchors {
            top: toolbar_1.bottom
            left: parent.left
        }
        height: 40
        spacing: toolbar_1.spacing

        Label {
            text: "length:"
            anchors.verticalCenter: length_field.verticalCenter
        }

        TextField {
            id: length_field
            validator: IntValidator {}
            text: root.loop.length.toString()
            onEditingFinished: {
                root.loop.set_length(parseInt(text))
                text = Qt.binding(() => root.loop.length.toString())
            }
        }

        ExtendedButton {
            tooltip: "Additional options."
            height: 35
            width: 30
            onClicked: { console.log ("Unimplemented") }

            anchors.verticalCenter: length_field.verticalCenter

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'dots-vertical'
                color: Material.foreground
            }
        }
    }

    Item {
        anchors {
            top: toolbar_2.bottom
            bottom: parent.bottom
            left: parent.left
            right: parent.right
        }

        Column {
            anchors.fill: parent
            Mapper {
                model: loop.channels

                ChannelDataRenderer {
                    property var mapped_item
                    property int index

                    height: 100
                    anchors {
                        left: parent.left
                        right: parent.right
                    }

                    channel: mapped_item
                    fetch_active: true
                    samples_per_pixel: zoom_slider.samples_per_pixel
                }
            }
        }

        // Render a scroll bar
        ScrollBar {
            id: scroll
            anchors {
                bottom: parent.bottom
                left: parent.left
                right: parent.right
            }
            size: {
                if(!fetcher.channel_data) { return 1.0 }
                var window_in_samples = width * root.samples_per_pixel
                return window_in_samples / fetcher.channel_data.length
            }
            minimumSize: 0.05
            orientation: Qt.Horizontal
            policy: ScrollBar.AlwaysOn
            visible: size < 1.0
        }
    }    
}