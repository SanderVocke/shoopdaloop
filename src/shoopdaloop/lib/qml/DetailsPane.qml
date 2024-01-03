import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

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

    property var items : {
        var rval = []
        user_items.forEach(u => {
            rval.push({'title': u.title, 'item': u.item, 'autoselect': u.autoselect, 'closeable': true})
        })
        temporary_items.forEach(t => {
            rval.push({'title': t.title, 'item': t.item, 'autoselect': t.autoselect, 'closeable': false})
        })
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
            model : root.items && root.items.length > 0 ?
                root.items :
                [{'title': '', 'item': undefined, 'autoselect': false, 'closeable': false}]
            
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

        StackLayout {
            id: stack
            anchors.fill: parent
            currentIndex: details_tabbar.currentIndex + 1 // +1 because of Mapper taking the first spot

            Mapper {
                model : root.items

                Item {
                    id: details_item
                    property var mapped_item
                    property int index

                    width: stack.width
                    height: stack.height

                    Loader {
                        anchors.fill: parent
                        anchors.margins: 5
                        property var maybe_loop_with_backend :
                            (details_item.mapped_item.item && details_item.mapped_item.item instanceof LoopWidget && details_item.mapped_item.item.maybe_backend_loop) ?
                            details_item.mapped_item.item : null
                        
                        active: maybe_loop_with_backend != null
                        sourceComponent: Component {
                            id: loop_content_widget
                            LoopContentWidget {
                                loop: parent.maybe_loop_with_backend
                                sync_loop: parent.maybe_loop_with_backend.sync_loop
                                anchors.fill: parent
                            }
                        }
                    }
                }
            }
        }
    }
}
