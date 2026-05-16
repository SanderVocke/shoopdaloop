import QtQuick 6.6
import ShoopDaLoop.Rust

Item {
    property var logger : ShoopRustLogger { name: "Frontend.Qml.SelectedLoops" }

    RegistryLookup {
        id: selected_loop_ids_lookup
        registry: AppRegistries.state_registry
        key: 'selected_loop_ids'
    }
    property var selected_loop_ids : selected_loop_ids_lookup.object ? selected_loop_ids_lookup.object : new Set()

    Connections {
        target: AppRegistries.state_registry
        function onItemModified(id, val) {
            if (id === 'selected_loop_ids') {
                // Force re-evaluation by creating a new Set from the modified data
                var new_set = new Set(val)
                selected_loop_ids = new_set
            }
        }
    }

    RegistryLookups {
        id: selected_loops_lookup
        registry: AppRegistries.objects_registry
        keys: Array.from(selected_loop_ids)
    }
    property var loops : new Set(selected_loops_lookup.objects)
}