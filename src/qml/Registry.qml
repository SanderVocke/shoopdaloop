import QtQuick 6.6
import ShoopDaLoop.PythonLogger

// A registry is a simple key-value store which can be shared by reference
Item {
    property var registry_data: ({})
    property var verbose: false

    property PythonLogger logger: PythonLogger { name: "Frontend.Qml.Registry" }

    signal contentsChanged()
    signal itemAdded(var id, var item)
    signal itemModified(var id, var item)
    signal itemRemoved(var id)
    signal cleared()

    function register(id, object, overwrite=false) {
        if(id in registry_data && !overwrite) {
            logger && logger.throw_error("attempting to re-register existing key: " + id)
        }
        if(!(id in registry_data)) {
            logger && logger.debug(() => (`Registered: ${id} => ${object}`))
            registry_data[id] = object
            itemAdded(id, object)
            contentsChanged()
        } else if(overwrite && (id in registry_data)) {
            logger &&logger.debug(() => (`Overwrite: ${id}: ${registry_data[id]}  => ${object}`))
            registry_data[id] = object
            itemModified(id, object)
            contentsChanged()
        }
    }

    function unregister(id) {
        if(!(id in registry_data)) {
            return;
        }
        logger && logger.debug(() => ("Unregistered:" + id))
        delete registry_data[id]
        itemRemoved(id)
        contentsChanged()
    }

    function get(id) {
        if(!(id in registry_data)) {
            logger.throw_error("attempting to get non-existing key: " + id)
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
        const d = registry_data[id]
        if(d == val) {
            if (typeof d === 'object') { itemModified(id, val) }
            return;
        }
        logger.debug(() => (`Replacing ${id}: ${d} => ${val}`))
        registry_data[id] = val
        itemModified(id, val)
        contentsChanged()
    }

    function multi_replace(dict) {
        logger.debug(() => (`Multi replace: ${Object.keys(dict)}`))
        var n_changed = 0;
        for (const [key, val] of Object.entries(dict)) {
            if (registry_data[key] != val) {
                registry_data[key] = val
                itemModified(key, val)
                n_changed++
            } else if (typeof val === 'object') {
                itemModified(key, val)
            }
        }
        if (n_changed > 0) {
            contentsChanged()
        }
    }

    function add_to_set(id, val) {
        if (!has(id)) { registry_data[id] = new Set() }
        mutate(id, s => { s.add(val); return s } )
        logger.debug(() => (`set contents after add: ${registry_data[id]}`))
    }

    function remove_from_set(id, val) {
        if (!has(id)) { registry_data[id] = new Set() }
        mutate(id, s => { s.delete(val); return s } )
        logger.debug(() => (`set contents after remove: ${registry_data[id]}`))
    }

    function clear_set(id) {
        replace(id, new Set())
    }

    function mutate(id, fn) {
        const mutated = fn(registry_data[id])
        if(verbose) {
            logger.debug(() => (`Mutating: ${id}, ${registry_data[id]} => ${mutated}`))
        }
        registry_data[id] = mutated
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

    function select_values(fn) {
        var r = []
        for (const [key, value] of Object.entries(registry_data)) {
            if(fn(value)) {
                r.push(value)
            }
        }
        return r
    }

    function all_values() {
        return Object.values(registry_data)
    }

    function keys() {
        return Object.keys(registry_data)
    }

    function clear(except_keys=[]) {
        var replace = {}
        for(const key of except_keys) { replace[key] = registry_data[key] }
        logger.debug(() => ("Clearing"))
        for(var key of Object.keys(registry_data)) {
            itemRemoved(key)
        }
        registry_data = replace
        contentsChanged()
        cleared()
    }

    function value_or(id, val) {
        if (has(id)) { return registry_data[id] }
        return val;
    }

    function generate_id(prefix) {
        for (var i = 0; ; i++) {
            var candidate = prefix + "_" + i.toString()
            if (!has(candidate)) {
                return candidate
            }
        }
    }
}