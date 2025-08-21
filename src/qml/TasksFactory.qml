import QtQuick 6.6

Item {
    function create_tasks_obj(parent) {
        return Qt.createQmlObject(`
            import QtQuick 6.6
            import ShoopDaLoop.Rust

            AsyncTasks {
            }
            `,
            parent
        );
    }

     function create_task_obj(parent) {
        return Qt.createQmlObject(`
            import QtQuick 6.6
            import ShoopDaLoop.Rust

            AsyncTask {
            }
            `,
            parent
        );
    }
}