import QtQuick 6.6
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
    function set_sync_active(val) {
        replace('sync_active', val)
    }

    RegistryLookup {
        id: lookup_solo_active
        registry: root
        key: 'solo_active'
    }
    readonly property bool solo_active : lookup_solo_active.object != null ? lookup_solo_active.object : false
    function set_solo_active(val) {
        replace('solo_active', val)
    }

    RegistryLookup {
        id: lookup_play_after_record_active
        registry: root
        key: 'play_after_record_active'
    }
    readonly property bool play_after_record_active : lookup_play_after_record_active.object != null ? lookup_play_after_record_active.object : false
    function set_play_after_record_active(val) {
        replace('play_after_record_active', val)
    }

    RegistryLookup {
        id: lookup_default_recording_action
        registry: root
        key: 'default_recording_action'
    }
    readonly property string default_recording_action : lookup_default_recording_action.object != null ? lookup_default_recording_action.object : 'record'
    function toggle_default_recording_action() {
        replace('default_recording_action', default_recording_action == 'record' ? 'grab' : 'record')
    }
    function set_default_recording_action(v) {
        if (['record', 'grab'].includes(v)) { replace('default_recording_action', v); }
        else { my_logger.error(() => `Invalid default recording action: ${v}`) }
    }

    RegistryLookup {
        id: lookup_apply_n_cycles
        registry: root
        key: 'apply_n_cycles'
    }
    readonly property int apply_n_cycles : lookup_apply_n_cycles.object != null ? lookup_apply_n_cycles.object : 0
    function set_apply_n_cycles(val) {
        replace('apply_n_cycles', Math.max(val, 0))
    }

    RegistryLookup {
        id: lookup_details_open
        registry: root
        key: 'details_open'
    }
    readonly property int details_open : lookup_details_open.object != null ? lookup_details_open.object : false
    function set_details_open(val) {
        replace('details_open', val)
    }

    RegistryLookup {
        id: lookup_targeted_loop
        registry: root
        key: 'targeted_loop'
    }
    property alias targeted_loop : lookup_targeted_loop.object
    function set_targeted_loop(maybe_loop) {
        replace('targeted_loop', maybe_loop)
    }
    function untarget_loop() { set_targeted_loop(null) }

    onN_saving_actions_activeChanged: my_logger.debug(() => ('N saving actions active: ' + n_saving_actions_active))
    onN_loading_actions_activeChanged: my_logger.debug(() => ('N loading actions active: ' + n_loading_actions_active))

    function reset() {
        reset_saving_loading()
        set_details_open(false)
        set_apply_n_cycles(0)
        set_sync_active(true)
        set_play_after_record_active(true)
        set_solo_active(false)
        untarget_loop()
    }

    Component.onCompleted: reset()
    onCleared: reset()
}