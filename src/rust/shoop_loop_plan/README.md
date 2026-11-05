# Sparse loop program preparation

This `no_std` crate contains the allocating, control-side program compiler shared
by the portable loop library and production client adapters. It has no runtime,
application, device or carrier dependencies. Targets are generic ordered keys;
the caller supplies their immutable prepared lengths and validates the complete
dependency graph.

Sections run sequentially. Entries within a section run in parallel, with delays,
optional durations and repetitions measured in the prepared unit. The compiler
resolves overlaps deterministically into indexed target segments. Storage and
compilation depend on bounded authored entries and boundaries, not frame duration.

The loop library retains the runtime views, dependency validation and owning
topology transactions. Its independent scalar oracle, sparse-program, topology
staging and production engine image tests exercise this compiler. The production
adapter can prepare plans without depending on the render runtime; engine delivery
validates those plans again using prepared storage.
