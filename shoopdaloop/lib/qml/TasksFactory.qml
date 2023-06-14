import QtQuick 6.3

Item {
    function create_tasks_obj(parent) {
        return Qt.createQmlObject(`
            import QtQuick 6.3
            import Tasks

            Tasks {
            }
            `,
            parent
        );
    }

     function create_task_obj(parent) {
        return Qt.createQmlObject(`
            import QtQuick 6.3
            import Task

            Task {
            }
            `,
            parent
        );
    }
}