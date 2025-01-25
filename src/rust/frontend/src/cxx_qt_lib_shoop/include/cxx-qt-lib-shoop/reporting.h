#pragma once

inline void report_signal_not_found(QObject * obj, std::string name) {
    std::string err = "Could not find signal ";
    err += name;
    err += std::string(". Available signals:\n");
    for (int i=0; i<obj->metaObject()->methodCount(); i++) {
        if (obj->metaObject()->method(i).methodType() != QMetaMethod::Signal) {
            continue;
        }
        err += "  - ";
        err += std::string(obj->metaObject()->method(i).methodSignature());
        err += "\n";
    }
    throw std::runtime_error(err);
}

inline void report_method_not_found(QObject * obj, std::string method_name) {
    std::string err = "Could not find method ";
    err += method_name;
    err += std::string(". Available methods:\n");
    for (int i=0; i<obj->metaObject()->methodCount(); i++) {
        err += "  - ";
        err += std::string(
            QMetaObject::normalizedSignature(
                obj->metaObject()->method(i).methodSignature()
        ));
        err += "\n";
    }
    throw std::runtime_error(err);
}