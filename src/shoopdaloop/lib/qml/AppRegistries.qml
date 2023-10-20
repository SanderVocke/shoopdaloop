import QtQuick 6.3

Item {
    // The main registry stores various important internal states.
    // Examples of stuff stored in here:
    // - selected / targeted / master loops
    // - sub-registries, such as for cached FX chain states.
    property Registry state_registry: StateRegistry {
        verbose: false
    }

    // The objects registry is a simple map of widget ids (from the session descriptor)
    // to objects representing those in the app (usually widgets.)
    property Registry objects_registry: ObjectsRegistry {
        verbose: false
    }

    // Store a reference to ourselves in the state registry.
    // Also make some sub-registries:
    // - one to keep track of FX chain states caching
    // - one to keep track of any important objects by ID.
    property Registry fx_chain_states_registry: Registry { verbose: true }

    // Component.onCompleted: {
    //     // Store in global context
    //     registries.state_registry = state_registry
    //     registries.objects_registry = objects_registry
    //     registries.fx_chain_states_registry = fx_chain_states_registry
    // }
}