from PySide6.QtCore import QAbstractItemModel, QModelIndex, QObject, Slot, QByteArray
from PySide6.QtQml import QJSValue
import re

# A tree model which can be used with e.g. QML TreeView.
# The model is read-only and low-performance (not meant for large datasets).
# At construction, pass the data as a regular Python dict of dicts.
# The outer dict keys will be used as the tree item names.
# The inner dict keys shall be the roles and data for each item.
# Achieving the hierarchy is done by a separator string in the item names.
# For example: by supplying items "Root" and "Root.Something" with separator ".",
# the model will yield an item Something that is a child of Root.
class DictTreeModel(QAbstractItemModel):
    class Node:
        def __init__(self, name, data, parent=None):
            self.name = name
            self.children = []
            self.parent = parent
            self.data = data

        def row(self):
            if self.parent:
                return self.parent.children.index(self)
            return 0

    def __init__(self, data, tree_separator, parent=None):
        super(DictTreeModel, self).__init__(parent)
        self.root_node = DictTreeModel.Node('root', None, None)
        self.tree_separator = tree_separator
        self.role_names = []

        # Create nodes
        for [name,item_data] in data.items():
            item_data['display'] = name
            item = self.create_node(name)
            item.data = {}
            for [role, value] in item_data.items():
                if not role in self.role_names:
                    self.role_names.append(role)
                item.data[self.role_names.index(role)] = value

    def create_node(self, name):
        prev = self.root_node
        for part in name.split(self.tree_separator):
            matches = [c for c in prev.children if c.name == part]
            if len(matches) != 1:
                node = DictTreeModel.Node(part, None, prev)
                prev.children.append(node)
                prev = node
            else:
                prev = matches[0]
        return prev
    
    def node(self, index):
        return index.internalPointer()
    
    def data(self, index, role):
        print("data")
        d = self.node(index).data
        return (d[role] if role in d else None)

    def index(self, row, column, parent):
        print("index")
        obj = None
        if parent.isValid():
            obj = self.node(parent).children[row]
        else:
            obj = self.root_node.children[row]
        return self.createIndex(row, column, obj)

    def parent(self, child):
        print("parent")
        if not child.isValid():
            return QModelIndex()
        
        node = self.node(child)
        if node.parent == None or node.parent == self.root_node:
            return QModelIndex()

        return self.createIndex(node.row(), 0, node.parent)

    def rowCount(self, parent):
        print("rowCount")
        if not parent.isValid():
            return len(self.root_node.children)
        return len(self.node(parent).children)

    def columnCount(self, parent):
        print("columnCount")
        return 1
    
    def roleNames(self):
        print("roleNames")
        return {int(k): QByteArray(v) for k, v in enumerate(self.role_names)}

class DictTreeModelFactory(QObject):
    def __init__(self, parent=None):
        super(DictTreeModelFactory, self).__init__(parent)
    
    @Slot('QVariant', str, result='QVariant')
    def create(self, data, tree_separator):
        if isinstance(data, QJSValue):
            data = data.toVariant()
        rval = DictTreeModel(data, tree_separator, parent=self)
        return rval
