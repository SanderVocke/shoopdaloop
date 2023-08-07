#pragma once

class ExternalUIInterface {
public:
    ExternalUIInterface() {}
    virtual ~ExternalUIInterface() {}

    virtual bool visible() const;
    virtual void show();
    virtual void hide();
};