import QtQuick 6.3
import QtQuick.Window 6.3
import QtWayland.Compositor
import QtWayland.Compositor.XdgShell
import QtWayland.Compositor.WlShell
import QtWayland.Compositor.IviApplication

Item {
    id: root
    
    property var shellSurfaces: compositor_loader.item ? compositor_loader.item.shellSurfaces : []
    
    property string socketName: 'shoop-wayland-display-' + Math.random().toString(36).slice(2, 7)

    Component.onCompleted: {
        if (true) { //(qpa_platform == 'xcb') {
            console.log('FIX QPA PLATFORM CHECK')
            // On X11, we need to set some env vars before starting our compositor.
            env_helper.set_env('QT_XCB_GL_INTEGRATION', 'xcb_egl')
            env_helper.set_env('QT_WAYLAND_CLIENT_BUFFER_INTEGRATION', 'wayland-egl')
        }

        // We are already displaying, but our child processes will need to
        // find the socket and know it should run on Wayland.
        env_helper.set_env('QT_QPA_PLATFORM', 'wayland')
        env_helper.set_env('WAYLAND_DISPLAY', socketName)

        // Start the compositor.
        compositor_loader.active = true
    }

    function removeShellSurface(surface) {
        if (compositor_loader.item) {
            compositor_loader.item.shellSurfaces.remove(surface)
        }
    }

    Loader {
        id: compositor_loader
        active: false

        sourceComponent: Component {
            WaylandCompositor {
                id: compositor
                socketName: root.socketName
                WlShell {
                    onWlShellSurfaceCreated: (shellSurface) => shellSurfaces.append({shellSurface: shellSurface});
                }
                XdgShell {
                    onToplevelCreated: (toplevel, xdgSurface) => shellSurfaces.append({shellSurface: xdgSurface});
                }
                IviApplication {
                    onIviSurfaceCreated: (iviSurface) => shellSurfaces.append({shellSurface: iviSurface});
                }

                property var shellSurfaces: []

                WaylandOutput {
                    sizeFollowsWindow: true
                    window: { console.log(compositor_loader.Window.window); return compositor_loader.Window.window }
                }
            }
        }
    }
}