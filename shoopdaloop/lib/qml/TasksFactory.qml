import QtQuick 2.0

Item {
    function create_tasks_obj(parent) {
        console.log(parent)
        return Qt.createQmlObject(`
            import QtQuick 2.0
            import Tasks

            Tasks {
            }
            `,
            parent
        );
    }

     function create_task_obj(parent) {
        return Qt.createQmlObject(`
            import QtQuick 2.0
            import Task

            Task {
            }
            `,
            parent
        );
    }
}