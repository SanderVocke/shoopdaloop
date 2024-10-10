from PySide6.QtQuick import QQuickItem

# Traverse up the parent tree to find an item.
def findFirstParent(base_item, predicate):
    p = base_item.parentItem()
    if not p or not isinstance(p, QQuickItem):
        return None
    if predicate(p):
        return p
    return findFirstParent(p, predicate)