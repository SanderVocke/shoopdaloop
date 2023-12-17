import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonFetchChannelData

import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    property var channel
    property alias fetch_active: fetcher.active
    property real samples_per_pixel: 1.0
    property int samples_offset: 0.0
    property int loop_length: 0
    readonly property int n_samples_shown: width * samples_per_pixel
    readonly property int n_samples: fetcher && fetcher.channel_data ? fetcher.channel_data.length : 0
    property real major_grid_lines_interval
    property real minor_grid_lines_interval

    property var played_back_sample : channel ? channel.played_back_sample : 0
    property int n_preplay_samples : channel ? channel.n_preplay_samples : 0
    property int start_offset : channel ? channel.start_offset : 0

    function snap_sample_to_grid(sample_idx) {
        var interval = minor_grid_lines_interval ? minor_grid_lines_interval : major_grid_lines_interval
        if (interval != undefined) {
            var steps_from_start = Math.round((sample_idx - start_offset) / interval)
            return start_offset + steps_from_start * interval
        }
    }

    function snap_visual_to_grid(x) {
        return map_sample_to_pixel(snap_sample_to_grid(map_pixel_to_sample(x)))
    }

    function map_sample_to_pixel(s) {
        return Math.round((s - samples_offset) / samples_per_pixel);
    }

    function map_pixel_to_sample(p) {
        return Math.round(p * samples_per_pixel + samples_offset)
    }

    // Render data
    Loader {
        active: root.channel

        Rectangle {
            id: background_rect
            width: root.width
            height: root.height
            color: 'black'
            border.color: 'grey'
            border.width: 1

            Repeater {
                id: minor_grid_lines_repeater
                anchors.fill: parent

                property var at_pixels: {
                    if (!root.visible) { return [] }
                    if (root.minor_grid_lines_interval <= 0) { return [] }

                    root.samples_offset;    // explicit dependency
                    root.samples_per_pixel; // explicit dependency

                    var rval = []
                    var s = root.start_offset;
                    while (map_sample_to_pixel(s) >= 0) { s -= root.minor_grid_lines_interval }
                    s += root.minor_grid_lines_interval;
                    for(; map_sample_to_pixel(s) < width; s += root.minor_grid_lines_interval) {
                        rval.push(map_sample_to_pixel(s))
                    }

                    return rval
                }

                model: at_pixels

                Rectangle {
                    property var mapped_item
                    property int index

                    color: 'grey'
                    width: 1
                    height: root.height
                    property var val : minor_grid_lines_repeater.at_pixels[index]
                    x: val ? val : 0
                }
            }

            Repeater {
                id: major_grid_lines_repeater
                anchors.fill: parent

                property var at_pixels: {
                    if (!root.visible) { return [] }
                    if (root.major_grid_lines_interval <= 0) { return [] }

                    root.samples_offset;    // explicit dependency
                    root.samples_per_pixel; // explicit dependency

                    var rval = []
                    var s = root.start_offset;
                    while (map_sample_to_pixel(s) >= 0) { s -= root.major_grid_lines_interval }
                    s += root.major_grid_lines_interval;
                    for(; map_sample_to_pixel(s) < width; s += root.major_grid_lines_interval) {
                        rval.push(map_sample_to_pixel(s))
                    }

                    return rval
                }

                model: at_pixels.length

                Rectangle {
                    color: 'white'
                    width: 1
                    height: root.height
                    property var val : major_grid_lines_repeater.at_pixels[index]
                    x: val ? val : 0
                }
            }
        }
        
        RenderChannelData {
            id: render
            input_data: fetcher && fetcher.channel_data && root.visible ? fetcher.channel_data : []
            channel: root.channel
            samples_per_bin: root.samples_per_pixel
            samples_offset: root.samples_offset
            width: root.width
            height: root.height
            clip: true

            // Will repeatedly fetch channel data when changed.
            PythonFetchChannelData {
                id: fetcher
                channel: root.channel
            }

            Rectangle {
                id: loop_window_rect
                color: 'blue'
                width: (root.n_samples - root.start_offset) / render.samples_per_bin
                height: render.height
                opacity: 0.3
                x: (root.start_offset - render.samples_offset) / render.samples_per_bin
                y: 0
            }

            Rectangle {
                id: preplay_window_rect
                color: 'yellow'
                width: root.n_preplay_samples / render.samples_per_bin
                height: render.height
                opacity: 0.3
                x: (root.start_offset - render.samples_offset - root.n_preplay_samples) / render.samples_per_bin
                y: 0
            }

            Rectangle {
                id: unplayed_data_before_rect
                color: 'darkblue'
                width: (root.start_offset - root.n_preplay_samples) / render.samples_per_bin
                height: render.height
                opacity: 0.3
                x: -render.samples_offset / render.samples_per_bin
                y: 0
            }

            Rectangle {
                id: playback_cursor_rect
                visible: root.played_back_sample != null && root.played_back_sample != undefined
                color: 'green'
                width: 2
                height: render.height
                x: ((root.played_back_sample || 0) - render.samples_offset) / render.samples_per_bin
                y: 0
            }
        }
    }
}
