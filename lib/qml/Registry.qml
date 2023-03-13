import QtQuick 2.15

// A registry is a simple key-value store which can be shared by reference
QtObject {
    property var data: ({})
    property var verbose: false

    signal contentsChanged()
    signal itemAdded(var id, var item)
    signal itemModified(var id, var item)
    signal itemRemoved(var id)

    function register(id, object, overwrite=false) {
        if(id in data && !overwrite) {
            throw new Error("attempting to re-register existing key: " + id)
        }
        if(!(id in data)) {
            if (verbose) {
                console.log("REGISTRY: Registered:", id, " => ", object)
            }
            data[id] = object
            itemAdded(id, object)
            contentsChanged()
        } else if(overwrite && (id in data)) {
            if (verbose) {
                console.log("REGISTRY: Overwrite: ", id, ":", data[id], " => ", object)
            }
            data[id] = object
            itemModified(id, object)
            contentsChanged()
        }
    }

    function unregister(id) {
        if(!(id in data)) {
            return;
        }
        if(verbose) {
            console.log("REGISTRY: Unregistered:", id)
        }
        console.log('before', data[id])
        delete data[id]
        console.log('after', data[id])
        itemRemoved(id)
        contentsChanged()
    }

    function get(id) {
        if(!(id in data)) {
            throw new Error("attempting to get non-existing key: " + id)
        }
        return data[id]
    }

    function maybe_get(id, fallback) {
        if(!(id in data)) {
            return fallback
        }
        return data[id]
    }

    function has(id) {
        return id in data;
    }

    function replace(id, val) {
        if(data[id] == val) { return; }
        if(verbose) {
            console.log("REGISTRY: Replacing:", id, val)
        }
        data[id] = val
        itemModified(id, data[id])
        contentsChanged()
    }

    function mutate(id, fn, val) {
        data[id] = fn(data[id])
        if(verbose) {
            console.log("REGISTRY: Mutating:", id, " => ", data[id])
        }
        itemModified(id, data[id])
        contentsChanged()
    }

    function select(fn) {
        var r = []
        for (const [key, value] of Object.entries(data)) {
            if(fn(value)) {
                r.push([key, value])
            }
        }
        return r
    }

    function clear(except_keys=[]) {
        var replace = {}
        for(const key of except_keys) { replace[key] = data[key] }
        if(verbose) {
            console.log("REGISTRY: Clearing")
        }
        for(var key of Object.keys(data)) {
            itemRemoved(key)
        }
        data = replace
        contentsChanged()  
    }
}