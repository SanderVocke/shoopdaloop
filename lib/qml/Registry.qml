import QtQuick 2.15

// A registry is a simple key-value store which can be shared by reference
QtObject {
    property var data: ({})
    property var verbose: false

    signal contentsChanged()
    signal itemAdded(var id, var item)
    signal itemModified(var id)

    function register(id, object) {
        if(id in data) {
            throw new Error("attempting to re-register existing key: " + id)
        }
        if(verbose) {
            console.log("REGISTRY: Registered:", id, " => ", object)
        }
        data[id] = object
        itemAdded(id, object)
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
        itemModified(id)
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