import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtWayland.Compositor

Item {
    id: root

    property real max_height: 1000
    property real min_height: 50
    property alias pane_y_offset : details_tabbar.height

    // The details for items are structured as:
    // [ { 'title': str, 'item': item, 'autoselect': bool  } ]
    // They are passed in in two categories:
    // - temporary items are not closeable by the user and relate to the app state (e.g. selected loops)
    // - user items are open until the user closes them.
    property var temporary_items : []
    property var user_items: []
    property var builtin_itmes: carla_window_items

    RegistryLookup {
        id: lookup_carla_wayland_wrapper
        registry: registries.state_registry
        key: 'carla_wayland_wrapper'
    }
    property alias carla_wayland_wrapper : lookup_carla_wayland_wrapper.object

    property var carla_window_items: carla_wayland_wrapper && carla_wayland_wrapper.shellSurfaces ? carla_wayland_wrapper.shellSurfaces.map(s => ({
        'title': s.title ? s.title : 'Untitled Wayland Window',
        'item': s,
        'autoselect': false
    })) : []

    property var items : {
        var rval = []
        user_items.forEach(u => {
            rval.push({'title': u.title, 'item': u.item, 'autoselect': u.autoselect, 'closeable': true})
        })
        temporary_items.forEach(t => {
            rval.push({'title': t.title, 'item': t.item, 'autoselect': t.autoselect, 'closeable': false})
        })
        builtin_itmes.forEach(b => {
            rval.push({'title': b.title, 'item': b.item, 'autoselect': b.autoselect, 'closeable': false})
        })
        if (rval.length == 0) {
            rval.push({'title': '...', 'item': 'empty-placeholder', 'autoselect': true, 'closeable': false})
        }
        return rval
    }

    function add_user_item(title, item, select=true) {
        user_items.push({ 'title': title, 'item': item, 'autoselect': true })
        user_itemsChanged()
        if (select) {
            details_tabbar.select_representative(item)
        }
    }
    function remove_user_item(item) {
        user_items = user_items.filter(u => u.item != item)
        user_itemsChanged()
    }

    ShoopTabBar {
        id: details_tabbar
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }
        Mapper {
            model : root.items
            
            ShoopTabButton {
                property var mapped_item
                property int index
                text: mapped_item.title
                tabbar: details_tabbar
                representative: mapped_item.item
                closeable : mapped_item.closeable
                onClose: root.remove_user_item(mapped_item.item)
            }
        }
    }
    Rectangle {
        id: contentrect
        color: "#555555"
        anchors {
            top: details_tabbar.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }

        ScrollView {
            anchors.fill: parent
            ScrollBar.vertical.policy: ScrollBar.AsNeeded
            ScrollBar.horizontal.policy: ScrollBar.AsNeeded

            StackLayout {
                id: stack
                width: parent.width
                currentIndex: details_tabbar.currentIndex + 1 // +1 because of Mapper taking the first spot

                Mapper {
                    model : root.items

                    Item {
                        id: details_item
                        property var mapped_item
                        property int index

                        width: stack.width
                        height: childrenRect.height

                        property var maybe_loop : details_item.mapped_item.item && details_item.mapped_item.item instanceof LoopWidget
                        property var maybe_loop_with_backend :
                            (maybe_loop && details_item.mapped_item.item.maybe_backend_loop) ?
                            details_item.mapped_item.item : null
                        
                        Loader {
                            anchors {
                                left: parent.left
                                right: parent.right
                                leftMargin: 5
                                rightMargin: 5
                            }
                            active: details_item.mapped_item.item == 'empty-placeholder'
                            sourceComponent: Component {
                                Label {
                                    text: 'Make a selection to show additional details here.'
                                }
                            }
                        }

                        Loader {
                            anchors {
                                left: parent.left
                                right: parent.right
                                leftMargin: 5
                                rightMargin: 5
                            }
                            active: parent.maybe_loop && !parent.maybe_loop_with_backend
                            sourceComponent: Component {
                                Label {
                                    text: 'Details will be shown once the loop is recording.'
                                }
                            }
                        }

                        Loader {
                            anchors {
                                left: parent.left
                                right: parent.right
                                leftMargin: 5
                                rightMargin: 5
                            }
                            height: childrenRect.height

                            active: details_item.maybe_loop_with_backend != null
                            sourceComponent: Component {
                                id: loop_content_widget
                                LoopContentWidget {
                                    loop: details_item.maybe_loop_with_backend
                                    sync_loop: details_item.maybe_loop_with_backend.sync_loop
                                    width: parent.width
                                }
                            }
                        }

                        Loader {
                            anchors {
                                left: parent.left
                                right: parent.right
                                leftMargin: 5
                                rightMargin: 5
                            }
                            height: contentrect.height

                            active: (details_item.mapped_item.item instanceof WlShellSurface) ||
                                    (details_item.mapped_item.item instanceof IviSurface) ||
                                    (details_item.mapped_item.item instanceof XdgSurface)
                            
                            sourceComponent: Component {
                                ShellSurfaceItem {
                                    width: contentrect.width
                                    height: contentrect.height
                                    shellSurface: details_item.mapped_item.item
                                    onSurfaceDestroyed: root.carla_wayland_wrapper.removeShellSurface(shellSurface)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
