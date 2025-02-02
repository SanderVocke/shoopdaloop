Item {
    property var provider : parent
    property var backend: {
        if (!provider) { return null; }
        else {
            if (provider.backend) { return provider.backend; }
            else {
                console.log(`Error: component ${parent} needs a "back-end" property which is set.`)
            }
        }
    }
}