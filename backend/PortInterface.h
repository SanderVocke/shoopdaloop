#pragma once
#include <string>

enum class PortDirection {
    Input,
    Output
};

class PortInterface {
public:

    virtual void close() = 0;
    virtual PortDirection direction() const = 0;
    virtual std::string const& name() const = 0;

    PortInterface() {}
    virtual ~PortInterface() {};
};