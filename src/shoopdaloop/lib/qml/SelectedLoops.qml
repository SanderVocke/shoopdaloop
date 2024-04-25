import QtQuick 6.6

Item {
    RegistryLookup {
        id: selected_loop_ids_lookup
        registry: registries.state_registry
        key: 'selected_loop_ids'
    }
    property var selected_loop_ids : selected_loop_ids_lookup.object ? selected_loop_ids_lookup.object : new Set()

    RegistryLookups {
        id: selected_loops_lookup
        registry: registries.objects_registry
        keys: Array.from(selected_loop_ids)
    }
    property var loops : new Set(selected_loops_lookup.objects)
}