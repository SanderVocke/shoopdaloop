import QtQuick 6.3
import ShoopDaLoop.PythonLogger

Registry {
    id: root

    property PythonLogger my_logger : PythonLogger { name: "Frontend.Qml.StateRegistry" }

    function save_action_started() {
        my_logger.debug(() => ("Save action started"))
        mutate('n_saving_actions_active', (n) => { return n + 1 })
    }
    function save_action_finished() {
        my_logger.debug(() => ("Save action finished"))
        mutate('n_saving_actions_active', (n) => { return n - 1 })
    }
    function load_action_started() {
        my_logger.debug(() => ("Load action started"))
        mutate('n_loading_actions_active', (n) => { return n + 1 })
    }
    function load_action_finished() {
        my_logger.debug(() => ("Load action finished"))
        mutate('n_loading_actions_active', (n) => { return n - 1 })
        }
    function reset_saving_loading() {
        my_logger.debug(() => ("Resetting saving and loading actions"))
        replace('n_loading_actions_active', 0)
        replace('n_saving_actions_active', 0)
    }

    RegistryLookup {
        id: lookup_saving
        registry: root
        key: 'n_saving_actions_active'
    }
    RegistryLookup {
        id: lookup_loading
        registry: root
        key: 'n_loading_actions_active'
    }
    property alias n_saving_actions_active : lookup_saving.object
    property alias n_loading_actions_active : lookup_loading.object

    RegistryLookup {
        id: lookup_sync_loop
        registry: root
        key: 'sync_loop'
    }
    property alias sync_loop : lookup_sync_loop.object

    RegistryLookup {
        id: lookup_sync_active
        registry: root
        key: 'sync_active'
    }
    readonly property bool sync_active : lookup_sync_active.object != null ? lookup_sync_active.object : false

    onN_saving_actions_activeChanged: my_logger.debug(() => ('N saving actions active: ' + n_saving_actions_active))
    onN_loading_actions_activeChanged: my_logger.debug(() => ('N loading actions active: ' + n_loading_actions_active))

    function init() {
        register('n_loading_actions_active', 0)
        register('n_saving_actions_active', 0)
    }

    Component.onCompleted: init()
    onCleared: init()
}