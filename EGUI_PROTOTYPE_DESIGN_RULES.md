# Design Rules for egui Prototype Components

These rules apply to egui prototype components in the `shoop_egui` crate.

## Keep `shoop_egui` independent of Qt

`shoop_egui` must remain unaware of Qt, QML, and their integration mechanisms. Its components should accept plain input state and expose their output as Rust values or events so that the crate can later be reused in a pure egui GUI.

The `frontend` crate and QML layer own the Qt integration. They translate between QML-facing state and `shoop_egui` input models, and connect component output to the QML world through signals and slots.

## Keep business logic outside `shoop_egui`

`shoop_egui` is a presentation crate and must not perform application business logic. Components may manage local UI state, render caller-provided application state, and report user intent.

For example, spawning a loop is business logic because it creates a backend object. A component requesting this operation must emit a command event instead. The integrating layer handles the command, updates the backend, and supplies the resulting state to the component.

## Remain compatible with browser rendering

`shoop_egui` must be written so that it can be rendered in a browser. Use platform-independent egui APIs and browser-compatible dependencies. Components must render within a UI surface supplied by their host and must not create native windows or rely directly on native windowing facilities.

## Component boundary

An egui prototype component should therefore follow this flow:

1. Receive plain state from its host.
2. Render that state and manage only local presentation state.
3. Return typed actions or command events that describe user intent.
4. Let the host perform Qt integration, business logic, backend changes, and platform-specific window management.
