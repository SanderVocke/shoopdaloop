import QtQuick 2.15

Registry {
    function save_action_started() { mutate('n_saving_actions_active', (n) => { return n + 1 }) }
    function save_action_finished() { mutate('n_saving_actions_active', (n) => { return n - 1 }) }
    function load_action_started() { mutate('n_loading_actions_active', (n) => { return n + 1 }) }
    function load_action_finished() { mutate('n_loading_actions_active', (n) => { return n - 1 }) }

    Component.onCompleted: {
        register('n_loading_actions_active', 0)
        register('n_saving_actions_active', 0)
    }
}