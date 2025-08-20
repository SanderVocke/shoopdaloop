import QtQuick 6.6
import QtQuick.Controls.Material 6.6
import ShoopDaLoop.Rust

import ".."

ApplicationWindow {
    id: root

    Material.theme: Material.Dark

    Rectangle {
        color: Material.background
        anchors.fill: parent
    }

    Component.onCompleted: {
        let resource_dir = ShoopFileIO.get_resource_directory();
        let icon_dir = `${resource_dir}/iconset/icon_128x128.png`;
        ShoopGlobalUtils.set_window_icon_path(root, icon_dir)
        screen_grabber.add_window(root)
    }
    Component.onDestruction: {
        screen_grabber.remove_window(root)
    }
}