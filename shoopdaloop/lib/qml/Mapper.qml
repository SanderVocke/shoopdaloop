import QtQuick 2.15
import QtQuick.Controls 2.15

// A mapper is similar to a Repeater, but:
// - the model is an array as opposed to an integer.
// - the delegate has a property "mapped_item" available, as well as the "index".
//   the mapped item is an element from the array.
// - when the array changes, the mapper attempts to keep existing mapped delegates
//   alive, instead of re-constructing its children. This includes when they are
//   re-ordered.
Item {
    id: root
    property list<var> model : []
    default property Component delegate : Item {}

    property list<var> instances : []

    function instantiate_delegate(model_elem, index) {
        return delegate.createObject(root.parent, {
            mapped_item : model_elem,
            index : index
        })
    }

    onModelChanged: {
        var old_instances = [...instances]
        var new_instances = []

        for (var idx=0; idx<model.length; idx++) {
            var elem = model[idx]
            var maybe_existing_instance_idx = old_instances.findIndex((i) => i.mapped_item == elem)
            if (maybe_existing_instance_idx >= 0) {
                new_instances.push(old_instances[maybe_existing_instance_idx])
            } else {
                new_instances.push(instantiate_delegate(elem, idx))
            }
        }

        instances = [...new_instances]

        // Now re-insert the instances as children of our parent
        var filtered_parent_children = []
        var new_parent_children = []
        var old_parent_children = []
        for(var idx=0; idx<root.parent.children.length; idx++) { old_parent_children.push(root.parent.children[idx]) }
        for(var cidx = 0; cidx < root.parent.children.length; cidx++) {
            if (old_instances.find(e => e == root.parent.children[cidx]) == undefined) {
                filtered_parent_children.push (root.parent.children[cidx]);
            }
        }
        for(var cidx = 0; cidx < filtered_parent_children.length; cidx++) {
            new_parent_children.push(filtered_parent_children[cidx])
            if (filtered_parent_children[cidx] == this) {
                for (var i of new_instances) { new_parent_children.push(i) }
            }
        }
        for(var idx=0; idx<new_instances.length; idx++) {
            new_instances[idx].index = idx;
        }

        root.parent.children=new_parent_children;
    }
}