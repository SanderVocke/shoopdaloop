// Copyright (C) 2017 The Qt Company Ltd.
// SPDX-License-Identifier: LicenseRef-Qt-Commercial OR BSD-3-Clause

import QtQuick
import QtQuick.Window
import QtWayland.Compositor
import QtWayland.Compositor.XdgShell
import QtWayland.Compositor.WlShell
import QtWayland.Compositor.IviApplication

import Helper

//! [compositor]
Window {
    id: mainwindow
    width: 1024
    height: 768
    visible: true

    WaylandCompositor {
    //! [compositor]
        // The output defines the screen.

        socketName: 'testsocket'

        //! [output]
        WaylandOutput {
            sizeFollowsWindow: true
            window: mainwindow.Window.window
        }

        // Extensions are additions to the core Wayland
        // protocol. We choose to support three different
        // shells (window management protocols). When the
        // client creates a new shell surface (i.e. a window)
        // we append it to our list of shellSurfaces.

        function add_surface(surface) {
            shellSurfaces.append({shellSurface: surface})
        }

        //! [shells]
        WlShell {
            onWlShellSurfaceCreated: (shellSurface) => add_surface(shellSurface)
        }
        XdgShell {
            onToplevelCreated: (toplevel, xdgSurface) => add_surface(xdgSurface)
        }
        IviApplication {
            onIviSurfaceCreated: (iviSurface) => add_surface(shellSurface)
        }
        //! [shells]

        //! [model]
        ListModel { id: shellSurfaces }
        //! [model]

        Helper { id: helper }

        Component.onCompleted: {
            console.log('socket:', socketName)
            helper.set_env("WAYLAND_DISPLAY", socketName)
            helper.set_env("QT_QPA_PLATFORM", "wayland")
            helper.initialized_callback()
        }
    }

    //! [shell surface item]
    Rectangle {
        anchors.fill: parent
        color: 'black'
        anchors.margins: 10
    
        Item {
            anchors.fill: parent
            anchors.margins: 10
            Repeater {
                model: shellSurfaces
                // ShellSurfaceItem handles displaying a shell surface.
                // It has implementations for things like interactive
                // resize/move, and forwarding of mouse and keyboard
                // events to the client process.
                ShellSurfaceItem {
                    shellSurface: modelData
                    onSurfaceDestroyed: shellSurfaces.remove(index)
                }
            }
            //! [shell surface item]
        }
    }
}