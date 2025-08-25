import QtQuick 6.6
import ShoopDaLoop.PythonLogger

Registry {
    id: root

    property PythonLogger my_logger : PythonLogger { name: "Frontend.Qml.StateRegistry" }

    function update_active_io() {
        if (root.active_io_task && !root.active_io_task.active) {
            my_logger.debug("IO task finished")
            replace('active_io_task', null)
        }
    }
    function set_active_io_task_fn(create_task_fn) {
        if (root.active_io_task) {
            my_logger.error("Already have an I/O task active.")
            return null
        }
        var task = create_task_fn()
        my_logger.debug(`Setting active IO task: ${task}. Active: ${task.active}`)
        if (task.active) {
            replace('active_io_task', task)
        } else {
            my_logger.debug("IO task was already finished")
            replace('active_io_task', null)
        }
        update_active_io()
        return task;
    }
    function reset_active_io() {
        my_logger.debug("Resetting IO task status")
        replace('active_io_task', null)
    }

    RegistryLookup {
        id: lookup_active_io_task
        registry: root
        key: 'active_io_task'
    }
    property alias active_io_task : lookup_active_io_task.object
    Connections {
        target: root.active_io_task
        function onActiveChanged() {
            root.update_active_io()
        }
    }

    RegistryLookup {
        id: lookup_sync_loop
        registry: root
        key: 'sync_loop'
    }
    property alias sync_loop : lookup_sync_loop.object
    onSync_loopChanged: my_logger.debug(`Sync loop: ${sync_loop}`)

    RegistryLookup {
        id: lookup_sync_active
        registry: root
        key: 'sync_active'
    }
    readonly property bool sync_active : lookup_sync_active.object != null ? lookup_sync_active.object : false
    function set_sync_active(val) {
        replace('sync_active', val)
    }
    onSync_activeChanged: my_logger.debug(`Sync active: ${sync_active}`)

    RegistryLookup {
        id: lookup_solo_active
        registry: root
        key: 'solo_active'
    }
    readonly property bool solo_active : lookup_solo_active.object != null ? lookup_solo_active.object : false
    function set_solo_active(val) {
        replace('solo_active', val)
    }
    onSolo_activeChanged: my_logger.debug(`Solo active: ${solo_active}`)

    RegistryLookup {
        id: lookup_play_after_record_active
        registry: root
        key: 'play_after_record_active'
    }
    readonly property bool play_after_record_active : lookup_play_after_record_active.object != null ? lookup_play_after_record_active.object : false
    function set_play_after_record_active(val) {
        replace('play_after_record_active', val)
    }
    onPlay_after_record_activeChanged: my_logger.debug(`Play after record active: ${play_after_record_active}`)

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
        else { my_logger.error(`Invalid default recording action: ${v}`) }
    }
    onDefault_recording_actionChanged: my_logger.debug(`Default recording action: ${default_recording_action}`)

    RegistryLookup {
        id: lookup_apply_n_cycles
        registry: root
        key: 'apply_n_cycles'
    }
    readonly property int apply_n_cycles : lookup_apply_n_cycles.object != null ? lookup_apply_n_cycles.object : 0
    function set_apply_n_cycles(val) {
        replace('apply_n_cycles', Math.max(val, 0))
    }
    onApply_n_cyclesChanged: my_logger.debug(`Apply N cycles: ${apply_n_cycles}`)

    RegistryLookup {
        id: lookup_details_open
        registry: root
        key: 'details_open'
    }
    readonly property int details_open : lookup_details_open.object != null ? lookup_details_open.object : false
    function set_details_open(val) {
        replace('details_open', val)
    }
    onDetails_openChanged: my_logger.debug(`Details open: ${details_open}`)

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
    onTargeted_loopChanged: my_logger.debug(`Targeted loop: ${targeted_loop}`)
    onActive_io_taskChanged: my_logger.debug('active IO task: ' + active_io_task)

    function reset() {
        reset_active_io()
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