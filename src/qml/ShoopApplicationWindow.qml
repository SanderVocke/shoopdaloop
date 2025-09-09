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
        let resource_dir = global_args.resource_dir;
        let icon_dir = `${resource_dir}/iconset/icon_128x128.png`;
        ShoopRustGlobalUtils.set_window_icon_path(root, icon_dir)
        ShoopRustTestScreenGrabber.add_window(root)
        ShoopRustKeyModifiers.install()
    }
    Component.onDestruction: {
        ShoopRustTestScreenGrabber.remove_window(root)
    }
}