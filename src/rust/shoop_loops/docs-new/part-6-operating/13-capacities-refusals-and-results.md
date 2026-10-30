# 13. Capacities, refusals, and results

[Contents](../README.md) · [Previous: Editing the graph around loops](../part-5-content-and-graph/12-editing-the-graph-around-loops.md) · [Next: Shutdown](14-shutdown.md)

Everything in `shoop_loops` is fixed-size: there is no allocation at render time, so the capacities you set at prepare time are the whole budget. When a budget is exhausted, you get a typed refusal — never a panic and never a silent drop.

## 1. The capacity knobs

| Knob | Where | What it limits |
|---|---|---|
| `Limits { loop_slots, pending_requests, .. }` | `compile` | Loops in the topology; pending requests (must exceed `Settings::normal_requests`). |
| `Settings { normal_requests, media_slots, export_slots }` | `LoopPlanner::prepare` | Control-side request slots; takes in the media ledger; export handles. |
| `Config { max_frames, max_nodes, max_ports, max_edges, resource_slots, resource_pages, page_bytes, command_slots, scheduled_events, .. }` | runtime | Host block size; graph size; resource identity/version/snapshot pools; sample pages; command and event slots. |
| `CaptureGroupConfig { groups, members_per_group, staging_bytes_per_group }` | resource store | Capture banks; each group's staging must hold `max_frames × channels × 4` bytes per audio member (or the MIDI event/byte budget). |
| `midi_events`, `midi_bytes`, `packet_bytes` | `Binding` | Per-take MIDI event/byte budget and per-packet render scratch. |

A practical rule from the examples: size staging for `max_frames`, not for the block you usually render, because a host may deliver any block up to `max_frames`.

## 2. The three result layers

Every submitted request produces up to two replies (chapter 3):

1. **Admission** (`try_completion`): the runtime accepted the command for delivery (`Scheduled`) or refused it at submission (e.g. full request slots — the refusal returns your command back).
2. **Execution** (`try_execution`): the musical result at the actual boundary — `Outcome::Applied { frame }`, `Superseded`, `Cancelled`, or `Failed { reason }`.
3. **Typed payload** (`operation_result`): `Reply::Transition(..)`, `Reply::Export { .. }`, `Reply::State(..)`, etc.

Common `Failed` reasons: `EmptyContent` (Play on an empty loop), `InvalidFrame` (position ≥ length), `InvalidState` (Seek while not playing, Replace while recording), `MissingSync` (`AfterTicks` whose source is not the target's sync source).

In a live application, poll both streams regularly and destroy returned owners on the control side; unread results occupy their finite slots. Match replies by `sequence`, not by order.

[Previous: Editing the graph around loops](../part-5-content-and-graph/12-editing-the-graph-around-loops.md) · [Next: Shutdown](14-shutdown.md)
