import QtQuick 6.6
import QtQuick.Controls 6.6

Item {
    id: root
    property var tasks: []

    property bool done: false
    readonly property bool active: !done
    property bool success: false

    property bool done_internal: false
    property bool success_internal: false

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.TaskObserver" }

    signal finished(bool success)

    ExecuteNextCycle {
        id: trigger_finished
        onExecute: finished(root.success)
    }

    onDoneChanged: {
        if (done) {
            root.logger.debug(`finished. success: ${root.success_internal}`)
            trigger_finished.trigger()
        }
    }

    function update() {
        var local_done = true;
        var local_success = true;
        for (var i=0; i<tasks.length; i++) {
            if (tasks[i].active) {
                local_done = false;
                break;
            } else if (tasks[i] !== undefined && tasks[i] !== null && !tasks[i].success) {
                console.log(i, tasks[i].active, tasks[i].success, tasks[i].bla)
                console.log(tasks[i])
                local_success = false;
            }
        }
        root.logger.trace(`updated. success: ${local_success}, done: ${local_done}`)
        root.success_internal = local_success
        root.done_internal = local_done
    }

    function start() {
        root.logger.debug(`start`)
        update()
        root.success = Qt.binding(() => root.success_internal)
        root.done = Qt.binding(() => root.done_internal)
    }

    function add_task(task) {
        root.logger.debug(`add task: ${task}`)
        root.tasks.push(task)
        task.onActive_changed.connect(root.update)
        update()
    }
}