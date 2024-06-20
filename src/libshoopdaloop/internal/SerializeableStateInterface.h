#pragma once
#include <string>
#include <cstdint>

class SerializeableStateInterface {
public:
    SerializeableStateInterface() {}

    // Asynchronously deserialize the given state string into the object.
    virtual void deserialize_state(std::string str) = 0;

    // Serialize the object state into a string synchronously.
    // If the timeout expires, will throw.
    virtual std::string serialize_state(uint32_t timeout_ms) = 0;
};