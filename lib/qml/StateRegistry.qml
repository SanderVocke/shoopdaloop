import QtQuick 2.15

Registry {
    Component.onCompleted: {
        register('n_loading_actions_active', 0)
        register('n_saving_actions_active', 0)
    }
}