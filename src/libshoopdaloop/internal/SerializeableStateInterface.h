#pragma once
#include <string>

class SerializeableStateInterface {
public:
    SerializeableStateInterface() {}
    virtual ~SerializeableStateInterface() {}

    virtual void deserialize_state(std::string str);
    virtual std::string serialize_state();
};