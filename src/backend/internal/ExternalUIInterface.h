#pragma once

class ExternalUIInterface {
public:
    ExternalUIInterface() {}

    virtual bool visible() const = 0;
    virtual void show() = 0;
    virtual void hide() = 0;
};