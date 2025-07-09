import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6
import QtGraphs
import QtQuick.Layouts
import ShoopDaLoop.PythonLogger

ApplicationWindow {
    id: root
    title: "Monitor"

    width: 600
    height: 450

    Material.theme: Material.Dark

    function add_dsp_load_point(load) {
        dsp_load.add_point(load)
    }
    function add_audio_buffer_pool_point(buffers_created, buffers_available) {
        audio_buffer_pool.add_point(buffers_created, buffers_available)
    }
    function add_backend_refresh_interval_point(interval) {
        backend_refresh_rate.add_point(interval)
    }

    ShoopComboBox {
        id: combo
        anchors.top: parent.top
        anchors.left: parent.left
        width: 200

        model: ["DSP load", "Audio buffer pool", "Backend state refresh rate"]
    }

    StackLayout {
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            top: combo.bottom
        }
        currentIndex: combo.currentIndex

        MonitorDspLoadGraph {
            id: dsp_load
        }
        MonitorAudioBufferPool {
            id: audio_buffer_pool
        }
        MonitorBackendRefreshRate {
            id: backend_refresh_rate
        }
    }
}
