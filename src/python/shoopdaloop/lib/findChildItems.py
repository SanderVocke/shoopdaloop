from PySide6.QtQuick import QQuickItem

# Regular findChildren does not traverse the complete QML
# tree. This function traverses the visual tree instead.
# Only works on QtQuickItems.
def findChildItems(base_item, predicate):
    rval = []
    for child in base_item.childItems():
        rval = rval + findChildItems(child, predicate)
        if predicate(child):
            rval.append(child)
    return rval