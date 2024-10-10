from PySide6.QtCore import QAbstractItemModel, QModelIndex, QObject, Slot, QByteArray
from PySide6.QtQml import QJSValue
import re
from .ShoopPyObject import *

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

    def __init__(self, data, tree_separator, column_roles, column_headers, parent=None):
        super(DictTreeModel, self).__init__(parent)
        self.root_node = DictTreeModel.Node('root', None, None)
        self.tree_separator = tree_separator
        self.column_roles = column_roles
        self.column_headers = column_headers

        # Create nodes
        item = self.create_node("Headers")
        item.data = {k : self.column_headers[i] for i,k in enumerate(self.column_roles)}
        for [name,item_data] in data.items():
            item_data['name'] = name.split(self.tree_separator)[-1]
            item = self.create_node(name)
            item.data = {}
            for [role, value] in item_data.items():
                item.data[role] = value

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
        if role == 0: # display
            d = self.node(index).data
            text = d[self.column_roles[index.column()]]
            return text
        return ''

    def index(self, row, column, parent):
        obj = None
        if parent.isValid():
            obj = self.node(parent).children[row]
        else:
            obj = self.root_node.children[row]
        return self.createIndex(row, column, obj)

    def parent(self, child):
        if not child.isValid():
            return QModelIndex()
        
        node = self.node(child)
        if node.parent == None or node.parent == self.root_node:
            return QModelIndex()

        return self.createIndex(node.row(), child.column(), node.parent)

    def rowCount(self, parent):
        if not parent.isValid():
            return len(self.root_node.children)
        return len(self.node(parent).children)

    def columnCount(self, parent):
        return len(self.column_roles)
    
    def roleNames(self):
        return {
            0: QByteArray('display')
        }

class DictTreeModelFactory(ShoopQObject):
    def __init__(self, parent=None):
        super(DictTreeModelFactory, self).__init__(parent)
    
    @ShoopSlot('QVariant', str, list, list, result='QVariant')
    def create(self, data, tree_separator, column_roles, column_headers):
        if isinstance(data, QJSValue):
            data = data.toVariant()
        rval = DictTreeModel(data, tree_separator, column_roles, column_headers, parent=self)
        return rval
