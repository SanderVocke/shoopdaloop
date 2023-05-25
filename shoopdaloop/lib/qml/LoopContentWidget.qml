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
        None,
        SetStartOffsetAll,
        SetStartOffsetSingle,
        SetEnd
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
                { value: LoopContentWidget.Tool.None, text: "none", all: false },
                { value: LoopContentWidget.Tool.SetStartOffsetAll, text: "set start (all)", all: true },
                { value: LoopContentWidget.Tool.SetStartOffsetSingle, text: "set start (single)", all: false },
                { value: LoopContentWidget.Tool.SetEnd, text: "set end (all)", all: true }
            ]

            property bool is_for_all_channels: model[currentIndex].all
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

        Item {
            id: channels_combine_range
            visible: false

            property int padding: 0.2 * 48000
            
            // Overall data starting point, where 0 is each channel's start offset.
            // That is to say: if three channels exist with start offsets 10, 20 and 30,
            // the one with offset 30 dominates so that the overall data starting point
            // becomes -30.
            property int data_start : -Math.max(...loop.channels.map(c => c.start_offset)) - padding
            
            // Likewise for the end point.
            property int data_end : Math.max(...loop.channels.map(c => c.data_length - c.start_offset)) + padding

            property int data_length : data_end - data_start
        }

        Column {
            anchors.fill: parent

            Mapper {
                id: channel_mapper
                model: loop.channels

                property var maybe_cursor_display_x: 0

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

                    property int first_pixel_sample: channel.start_offset + channels_combine_range.data_start

                    samples_offset: scroll.position * channels_combine_range.data_length + first_pixel_sample
                    loop_length: root.loop.length

                    property var maybe_cursor_display_x: {
                        if (hover_ma.containsMouse) { return hover_ma.mouseX; }
                        return channel_mapper.maybe_cursor_display_x
                    }

                    Rectangle {
                        width: 1
                        height: parent.height
                        color: 'white'
                        x : parent.maybe_cursor_display_x
                        visible: parent.maybe_cursor_display_x != null

                        Label {
                            anchors {
                                left: parent.left
                                top: parent.top
                                margins: 3
                            }
                            text: tool_combo.currentText
                        }
                    }

                    MouseArea {
                        id: hover_ma
                        enabled: tool_combo.currentValue != LoopContentWidget.Tool.None
                        anchors.fill: parent
                        hoverEnabled: enabled
                        cursorShape: enabled ? Qt.PointingHandCursor : null

                        onMouseXChanged: if(containsMouse &&
                                            tool_combo.is_for_all_channels) {
                            channel_mapper.maybe_cursor_display_x = mouseX
                        }
                    }
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
                var window_in_samples = width * zoom_slider.samples_per_pixel
                return window_in_samples / channels_combine_range.data_length
            }
            minimumSize: 0.05
            orientation: Qt.Horizontal
            policy: ScrollBar.AlwaysOn
            visible: size < 1.0
        }
    }    
}