import QtQuick 6.3

Item {
    function create_tasks_obj(parent) {
        return Qt.createQmlObject(`
            import QtQuick 6.3
            import ShoopDaLoop.PythonTasks

            PythonTasks {
            }
            `,
            parent
        );
    }

     function create_task_obj(parent) {
        return Qt.createQmlObject(`
            import QtQuick 6.3
            import ShoopDaLoop.PythonTask

            PythonTask {
            }
            `,
            parent
        );
    }
}