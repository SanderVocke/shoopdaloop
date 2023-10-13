import QtQuick 6.3

// Represent MIDI control configuration with a descriptor version number.
QtObject {
    // Note: don't set directly! Use from_dict instead.
    property var contents: []
    readonly property var current_version: 1

    function from_dict(dict) {
        while(dict['version'] != current_version) {
            dict = upgrade(dict)
        }
        contents = dict['configuration']
    }
    function to_dict() {
        return {
            'version': 1,
            'configuration': contents
        }
    }

    function upgrade(dict) {
        if (dict['version'] == 1) {
            return dict
        }
        throw new Error("Unrecognized MIDI control config version")
    }
}