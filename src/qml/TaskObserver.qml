import QtQuick 6.6
import QtQuick.Controls 6.6

QtObject {
    id: root
    property var tasks: []

    property bool done: false
    property bool success: false

    property bool done_internal: false
    property bool success_internal: false

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.TaskObserver" }

    signal finished()

    onDoneChanged: {
        if (done) {
            finished()
        }
    }

    function update() {
        var done = true;
        var success = true;
        for (var i=0; i<tasks.length; i++) {
            if (tasks[i].active) {
                done = false;
                break;
            } else if (!tasks[i].success) {
                success = false;
            }
        }
        root.logger.trace(`updated. success: ${success}, done: ${done}`)
        root.success_internal = success
        root.done_internal = done
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