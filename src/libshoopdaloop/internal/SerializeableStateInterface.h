#pragma once
#include <string>

class SerializeableStateInterface {
public:
    SerializeableStateInterface() {}

    virtual void deserialize_state(std::string str) = 0;
    virtual std::string serialize_state() = 0;
};