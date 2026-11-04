# Upstream source

Source: https://github.com/nannou-org/egui_graph
Revision: 1a5e2b5385c8c81a49cbde836317848b06849966 (0.19.1).
License: MIT; retained in LICENSE-MIT.

The repository manifest selects egui 0.37 and omits upstream examples/dev
dependencies. The source replaces deprecated AsId bounds with Hash + Debug,
Id::new with Id::unique, and Ui::id with Ui::scope_id. Shoop maintains this small
compatibility adaptation until a compatible upstream release is available.
