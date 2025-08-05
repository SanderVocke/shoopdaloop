import QtQuick 6.6
import ShoopDaLoop.Rust

import ".."

ApplicationWindow {
    id: root

    Component.onCompleted: {
        console.log("Let's go!")
        let resource_dir = file_io.get_resource_directory();
        let icon_dir = `${resource_dir}/iconset/icon_128x128.png`;
        ShoopGlobalUtils.set_window_icon_path(root, icon_dir)
    }
}