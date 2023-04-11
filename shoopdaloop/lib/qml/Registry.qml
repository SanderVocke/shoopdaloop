import QtQuick 2.15

// A registry is a simple key-value store which can be shared by reference
Item {
    property var registry_data: ({})
    property var verbose: false

    signal contentsChanged()
    signal itemAdded(var id, var item)
    signal itemModified(var id, var item)
    signal itemRemoved(var id)

    function register(id, object, overwrite=false) {
        if(id in registry_data && !overwrite) {
            throw new Error("attempting to re-register existing key: " + id)
        }
        if(!(id in registry_data)) {
            if (verbose) {
                console.log("REGISTRY: Registered:", id, " => ", object)
            }
            registry_data[id] = object
            itemAdded(id, object)
            contentsChanged()
        } else if(overwrite && (id in registry_data)) {
            if (verbose) {
                console.log("REGISTRY: Overwrite: ", id, ":", registry_data[id], " => ", object)
            }
            registry_data[id] = object
            itemModified(id, object)
            contentsChanged()
        }
    }

    function unregister(id) {
        if(!(id in registry_data)) {
            return;
        }
        if(verbose) {
            console.log("REGISTRY: Unregistered:", id)
        }
        console.log('before', registry_data[id])
        delete registry_data[id]
        console.log('after', registry_data[id])
        itemRemoved(id)
        contentsChanged()
    }

    function get(id) {
        if(!(id in registry_data)) {
            throw new Error("attempting to get non-existing key: " + id)
        }
        return registry_data[id]
    }

    function maybe_get(id, fallback) {
        if(!(id in registry_data)) {
            return fallback
        }
        return registry_data[id]
    }

    function has(id) {
        return id in registry_data;
    }

    function replace(id, val) {
        if(registry_data[id] == val) { return; }
        if(verbose) {
            console.log("REGISTRY: Replacing:", id, val)
        }
        registry_data[id] = val
        itemModified(id, registry_data[id])
        contentsChanged()
    }

    function multi_replace(dict) {
        if(verbose) {
            console.log("REGISTRY: Multi replace:", Object.keys(dict))
        }
        var n_changed = 0;
        for (const [key, val] of Object.entries(dict)) {
            if (registry_data[key] != val) {
                registry_data[key] = val
                itemModified(key, val)
                n_changed++
            }
        }
        if (n_changed > 0) {
            contentsChanged()
        }
    }

    function add_to_set(id, val) {
        var item = has(id) ? registry_data[id] : new Set()
        if (!item.has(val)) {
            item.add(val)
            registry_data[id] = item
            itemModified(id, item)
            contentsChanged()
        }
    }

    function remove_from_set(id, val) {
        var item = has(id) ? registry_data[id] : new Set()
        if (item.has(val)) {
            item.delete(val)
            registry_data[id] = item
            itemModified(id, item)
            contentsChanged()
        }
    }

    function clear_set(id) {
        replace(id, new Set())
    }

    function mutate(id, fn, val) {
        registry_data[id] = fn(registry_data[id])
        if(verbose) {
            console.log("REGISTRY: Mutating:", id, " => ", registry_data[id])
        }
        itemModified(id, registry_data[id])
        contentsChanged()
    }

    function select(fn) {
        var r = []
        for (const [key, value] of Object.entries(registry_data)) {
            if(fn(value)) {
                r.push([key, value])
            }
        }
        return r
    }

    function clear(except_keys=[]) {
        var replace = {}
        for(const key of except_keys) { replace[key] = registry_data[key] }
        if(verbose) {
            console.log("REGISTRY: Clearing")
        }
        for(var key of Object.keys(registry_data)) {
            itemRemoved(key)
        }
        registry_data = replace
        contentsChanged()  
    }

    function value_or(id, val) {
        if (has(id)) { return registry_data[id] }
        return val;
    }
}