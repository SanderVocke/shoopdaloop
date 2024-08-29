import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import ShoopDaLoop.PythonLogger

import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    property var loop
    property var sync_loop

    property PythonLogger logger : default_logger

    height: childrenRect.height

    enum Tool {
        None,
        SetStartOffsetAll,
        SetStartOffsetSingle,
        SetEnd,
        SetPreplaySamplesAll,
        SetPreplaySamplesSingle
    }

    function tool_clicked() {
        for (var idx=0; idx < channel_mapper.unsorted_instances.length; idx++) {
            var c = channel_mapper.unsorted_instances[idx]
            var channel = c.channel
            var s = c.render.maybe_cursor_sample_idx
            if (s) {
                switch (tool_combo.currentValue) {
                    case LoopContentWidget.Tool.None:
                        break;
                    case LoopContentWidget.Tool.SetStartOffsetAll:
                    case LoopContentWidget.Tool.SetStartOffsetSingle:
                        channel.set_start_offset(s);
                        break;
                    case LoopContentWidget.Tool.SetPreplaySamplesAll:
                    case LoopContentWidget.Tool.SetPreplaySamplesSingle:
                        var n = Math.max(0, channel.start_offset - s)
                        channel.set_n_preplay_samples(n)
                        break;
                    case LoopContentWidget.Tool.SetEnd:
                        var len = s - channel.start_offset
                        if (len >= 0) { channel.loop.set_length(len); }
                        else { root.logger.error(() => ("Ignoring invalid end point: is before start offset.")) }
                        break;
                    default:
                        root.logger.error(() => ("Unimplemented tool."))
                        throw new Error("Unimplemented")
                }
            }
        }
    }

    Row {
        id: toolbar
        property int tool_buttons_size: 24

        anchors {
            top: parent.top
            left: parent.left
        }
        height: 40
        spacing: 2

        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            material_design_icon: 'magnify'
            size: toolbar.tool_buttons_size

            onClicked: zoom_popup.open()
            toggle_visual_active: zoom_popup.visible

            Popup {
                id: zoom_popup
                leftInset: 20
                rightInset: 50
                topInset: 20
                bottomInset: 20

                ShoopSlider {
                    id: zoom_slider
                    width: 160
                    value: 8.0
                    from: 16.0
                    to: 0.0

                    property real prev_samples_per_pixel: 0.0
                    readonly property real samples_per_pixel: slider_to_samples_per_pixel(value)
                    Component.onCompleted: prev_samples_per_pixel = samples_per_pixel

                    onSamples_per_pixelChanged: {
                        let prev_n_samples = channels_column.width * prev_samples_per_pixel
                        let new_n_samples = channels_column.width * samples_per_pixel
                        let diff = new_n_samples - prev_n_samples
                        let new_offset = channels_column.samples_offset - diff / 2
                        prev_samples_per_pixel = samples_per_pixel

                        zoomed(samples_per_pixel, new_offset)
                    }

                    signal zoomed (real new_samples_per_pixel, real new_offset)

                    function samples_per_pixel_to_slider(spp) {
                        return Math.log2(spp)
                    }
                    function slider_to_samples_per_pixel(slider) {
                        return Math.pow(2.0, slider)
                    }
                    function set_samples_per_pixel(spp) {
                        value = samples_per_pixel_to_slider(spp)
                    }

                    anchors {
                        verticalCenter: parent.verticalCenter
                    }
                }
            }
        }

        property real zoom_step_amount : 0.05 * (zoom_slider.to - zoom_slider.from)
        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            material_design_icon: 'magnify-plus-outline'
            size: toolbar.tool_buttons_size

            onClicked: zoom_slider.value = Math.max(zoom_slider.value + toolbar.zoom_step_amount, zoom_slider.to)
        }

        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            material_design_icon: 'magnify-minus-outline'
            size: toolbar.tool_buttons_size

            onClicked: zoom_slider.value = Math.min(zoom_slider.value - toolbar.zoom_step_amount, zoom_slider.from)
        }

        ToolbarButton {
            anchors {
                verticalCenter: parent.verticalCenter
            }
            material_design_icon: 'magnify'
            size: toolbar.tool_buttons_size

            Label {
                x: 7
                y: 4
                text: "A"
                color: Material.foreground
                font.pixelSize: 8
            }

            onClicked: {
                let show_n_samples = channels_combine_range.data_length
                let margin_factor = 1.2
                let margin_samples = show_n_samples * (margin_factor - 1.0)
                zoom_slider.set_samples_per_pixel(show_n_samples * margin_factor / channels_column.width)
                channels_column.samples_offset = -channels_combine_range.data_start - margin_samples / 2
            }
        }
        
        ToolSeparator {
            anchors.top: parent.top
            anchors.bottom: parent.bottom
        }

        Label {
            anchors.verticalCenter: toolbar.verticalCenter
            text: "Tool:"
        }

        ShoopComboBox {
            id: tool_combo
            anchors.verticalCenter: toolbar.verticalCenter
            textRole: "text"
            valueRole: "value"
            width: 150

            model: [
                { value: LoopContentWidget.Tool.None, text: "none", all: false },
                { value: LoopContentWidget.Tool.SetStartOffsetAll, text: "set start (all)", all: true },
                { value: LoopContentWidget.Tool.SetStartOffsetSingle, text: "set start (single)", all: false },
                { value: LoopContentWidget.Tool.SetEnd, text: "set end (all)", all: true },
                { value: LoopContentWidget.Tool.SetPreplaySamplesAll, text: "set pre-play (all)", all: true },
                { value: LoopContentWidget.Tool.SetPreplaySamplesSingle, text: "set pre-play (single)", all: false },
            ]

            property bool is_for_all_channels: model[currentIndex].all
        }

        ToolSeparator {
            anchors.top: parent.top
            anchors.bottom: parent.bottom
        }

        Label {
            anchors.verticalCenter: toolbar.verticalCenter
            text: "Grid:"
        }

        ShoopComboBox {
            id: minor_grid_divider
            anchors.verticalCenter: toolbar.verticalCenter
            textRole: "text"
            valueRole: "value"
            currentIndex : 6
            width: 60

            model: [
                { value: undefined, text: "None" },
                { value: 1, text: "1/1" },
                { value: 2, text: "1/2" },
                { value: 3, text: "1/3" },
                { value: 4, text: "1/4" },
                { value: 6, text: "1/6" },
                { value: 8, text: "1/8" },
                { value: 16, text: "1/16" },
            ]
        }

        ToolbarButton {
            id: snap_switch
            text: "snap"
            anchors {
                verticalCenter: parent.verticalCenter
            }
            height: toolbar.tool_buttons_size
            togglable: true
        }

        ToolSeparator {
            anchors.top: parent.top
            anchors.bottom: parent.bottom
        }
    }

    Item {
        anchors {
            top: toolbar.bottom
            left: parent.left
            right: parent.right
        }

        height: childrenRect.height

        Item {
            id: channels_combine_range
            visible: false

            property int padding: 0.2 * 48000
            
            // Overall data starting point.
            // This number is calculated assuming that all channels' data is aligned such that
            // their start offsets are at 0. The overall start is then the earliest available
            // data from any of the channels, plus an extra padding margin.
            // Example: if three channels exist with start offsets 10, 20 and 30,
            // the one with offset 30 dominates so that the overall data starting point
            // becomes -30 plus padding.
            property int data_start : -Math.max(...loop.channels.map(c => c ? c.start_offset : 0)) - padding
            
            // Likewise for the end point.
            property int data_end : Math.max(...loop.channels.map(c => c ? c.data_length - c.start_offset : 0)) + padding

            property int data_length : data_end - data_start
        }

        Column {
            id: channels_column
            property real samples_offset : 0.0
            Mapper {
                id: channel_mapper
                model: loop.channels

                property var maybe_cursor_display_x: null

                ResizeableItem {
                    height: 50
                    width: root.width
                    visible: root.visible

                    property var mapped_item
                    property int index

                    max_height: 500
                    min_height: 20
                    bottom_drag_enabled: true
                    
                    property var render: renderer
                    property var channel: mapped_item

                    ChannelDataRenderer {
                        id: renderer
                        fetch_active: root.visible
                        anchors.fill: parent

                        channel: parent.channel

                        samples_per_pixel: 1.0
                        Component.onCompleted: samples_per_pixel = zoom_slider.samples_per_pixel

                        Connections {
                            target: zoom_slider
                            function onZoomed(spp, off) {
                                renderer.samples_per_pixel = spp;
                                channels_column.samples_offset = off
                            }
                        }

                        // The sample that aligns with the left-most pixel on screen.
                        // This 
                        property int first_pixel_sample: (channel ? channel.start_offset : 0) + channels_combine_range.data_start
                        samples_offset: first_pixel_sample + channels_column.samples_offset
                        onPan: (amount) => { channels_column.samples_offset += amount }

                        loop_length: root.loop ? root.loop.length : 0

                        property var maybe_cursor_display_x: {
                            var r = hover_ma.containsMouse ? hover_ma.mouseX : channel_mapper.maybe_cursor_display_x

                            if (r != undefined && r != null && snap_switch.checked) {
                                // Snap to grid
                                r = snap_visual_to_grid(r)                     
                            }

                            return r
                        }

                        property var maybe_cursor_sample_idx: maybe_cursor_display_x ?
                            samples_offset + maybe_cursor_display_x * samples_per_pixel :
                            null
                        
                        property bool enable_grid_lines: minor_grid_divider.currentValue != undefined && root.sync_loop && root.sync_loop.length >= 4800
                        major_grid_lines_interval: enable_grid_lines ? root.sync_loop.length : -1
                        minor_grid_lines_interval: enable_grid_lines && minor_grid_divider.currentValue > 1 ?
                                                    major_grid_lines_interval / minor_grid_divider.currentValue : -1

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
                            cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor

                            onMouseXChanged: if(containsMouse &&
                                                tool_combo.is_for_all_channels) {
                                channel_mapper.maybe_cursor_display_x = mouseX
                            }
                            onContainsMouseChanged: if (!containsMouse) { channel_mapper.maybe_cursor_display_x = null; }

                            onClicked: {
                                root.tool_clicked()
                            }
                        }
                    }
                }
            }
        }
    }    
}