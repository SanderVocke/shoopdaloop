import QtQuick 6.3
import QtQuick.Window 6.3
import QtWayland.Compositor
import QtWayland.Compositor.XdgShell
import QtWayland.Compositor.WlShell
import QtWayland.Compositor.IviApplication

import ShoopDaLoop.PythonLogger

Item {
    id: root
    
    property alias shellSurfaces: compositor.shellSurfaces
    
    property string socketName: 'shoop-wayland-display-' + Math.random().toString(36).slice(2, 7)

    property PythonLogger logger: PythonLogger { name: 'Frontend.Qml.CarlaWaylandWrapper' }

    Component.onCompleted: {
        // We are already displaying, but our child processes will need to
        // find the socket and know it should run on Wayland.
        root.logger.debug(`Wayland server for embedding subprocesses @ ${socketName}, setting QT_QPA_PLATFORM and WAYLAND_DISPLAY`)
        env_helper.set_env('QT_QPA_PLATFORM', 'wayland')
        env_helper.set_env('XDG_SESSION_TYPE', 'wayland')
        env_helper.set_env('GDK_BACKEND', 'wayland')
        env_helper.set_env('WAYLAND_DISPLAY', socketName)
    }

    function removeShellSurface(surface) {
        root.logger.debug(`Removing shell surface ${surface}`)
        compositor.shellSurfaces = compositor.shellSurfaces.filter(s => s != surface)
        shellSurfacesChanged()
    }

    WaylandCompositor {
        id: compositor
        socketName: root.socketName

        function add_surface(surface) {
            root.logger.debug(`Surface created for client ${surface.surface.client.processId}`)
            shellSurfaces.push(surface)
            shellSurfacesChanged()
        }

        WlShell {
            onWlShellSurfaceCreated: (shellSurface) => compositor.add_surface(shellSurface)
        }
        XdgShell {
            onToplevelCreated: (toplevel, xdgSurface) => {
                toplevel.onActivatedChanged.connect(() => { root.logger.trace(() => `XdgShell toplevel ${toplevel}: activated => ${toplevel.activated}`) })
                toplevel.onMinSizeChanged.connect(() => { root.logger.trace(() => `XdgShell toplevel ${toplevel}: minSize => ${toplevel.minSize}`) })
                toplevel.onMaxSizeChanged.connect(() => { root.logger.trace(() => `XdgShell toplevel ${toplevel}: maxSize => ${toplevel.maxSize}`) })
                toplevel.onParentToplevelChanged.connect(() => { root.logger.trace(() => `XdgShell toplevel ${toplevel}: parentToplevel => ${toplevel.parentToplevel}`) })

                root.logger.trace(() => `XdgShell toplevel ${toplevel} created: minSize ${toplevel.minSize}, maxSize ${toplevel.maxSize}, activated ${toplevel.activated}, parentToplevel ${toplevel.parentToplevel}`)

                compositor.add_surface(xdgSurface)
            }
        }
        IviApplication {
            onIviSurfaceCreated: (iviSurface) =>  compositor.add_surface(iviSurface)
        }

        property var shellSurfaces: []

        WaylandOutput {
            sizeFollowsWindow: false
            window: root.Window.window
        }
    }
}