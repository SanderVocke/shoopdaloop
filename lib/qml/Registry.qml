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
        if(verbose && !(id in data)) {
            console.log("REGISTRY: Registered:", id, " => ", object)
            data[id] = object
            itemAdded(id, object)
        } else if(verbose && overwrite && (id in data)) {
            console.log("REGISTRY: Overwrite: ", id, ":", data[id], " => ", object)
            data[id] = object
            itemModified(id, object)
        }
       
        contentsChanged()
    }

    function unregister(id) {
        if(!(id in data)) {
            throw new Error("attempting to unregister non-existent key: " + id)
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

    function has(id) {
        return id in data;
    }

    function modify(id, fn) {
        if(verbose) {
            console.log("REGISTRY: Modifying:", id)
        }
        fn(data[id])
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
}