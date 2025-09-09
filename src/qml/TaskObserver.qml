import QtQuick 6.6
import QtQuick.Controls 6.6

Item {
    id: root

    property int n_tasks: 0
    property int n_done: 0
    property bool started: false

    property bool done: false
    readonly property bool active: !done
    property bool success: true

    property bool done_internal: n_done >= n_tasks

    readonly property ShoopRustLogger logger : ShoopRustLogger { name: "Frontend.Qml.TaskObserver" }

    signal finished(bool success)

    ExecuteNextCycle {
        id: trigger_finished
        onExecute: finished(root.success)
    }

    onDoneChanged: {
        if (done) {
            root.logger.debug(`finished. success: ${root.success}`)
            trigger_finished.trigger()
        }
    }

    onDone_internalChanged: if(root.started) { root.done = true }
    onStartedChanged: if(root.done_internal) { root.done = true }


    function add_task(task) {
        root.logger.debug(`add task: ${task}`)
        n_tasks = n_tasks + 1
        if (!task.active) {
            if (!task.success) { success = false; }
            n_done = n_done + 1
        } else {
            task.onActive_changed.connect(() => {
                if (!task.active) {
                    root.logger.debug(`task ${task} finished (success: ${task.success})`)
                    if (!task.success) { success = false; }
                    n_done = n_done + 1
                }
            })
        }
    }

    function start() {
        root.logger.debug("start")
        root.started = true
    }
}