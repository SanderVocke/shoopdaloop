import QtQuick 6.6
import QtQuick.Controls 6.6

Item {
    property var registry
    property var object
    property string key
    property bool enabled: true

    property var registered_key : undefined
    property var registered_object : undefined
    property var registered_registry : undefined

    function close() {
        enabled = false
        update()
    }

    function unregister() {
        if (registered_key != undefined && registered_registry) {
            registered_registry.unregister(registered_key)
            registered_key = undefined
            registered_object = undefined
            registered_registry = undefined
        }
    }

    function replace_object() {
        if (registry) {
            registry.replace(key, object)
            registered_key = key
            registered_object = object
            registered_registry = registry
        }
    }

    function update() {
        if (!enabled) {
            unregister()
            return;
        }
        if (registered_registry != undefined && registered_registry != registry) {
            unregister()
        }
        if (registered_key != undefined && registered_key != key) {
            unregister()
        }
        replace_object()
    }

    Component.onCompleted: update()
    Component.onDestruction: close()
    onRegistryChanged: update()
    onObjectChanged: update()
    onKeyChanged: update()
}