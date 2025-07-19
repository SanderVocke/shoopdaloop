#pragma once
#include <QQuickItem>


#include <iostream>

template<typename Item>
inline QQuickItem * maybe_parent_qquickitem(Item const& item) {
    std::cout << "Getting parent of " << &item << ": " << item.parent() << ", " << item.parentItem() << std::endl;
    return item.parentItem();
}