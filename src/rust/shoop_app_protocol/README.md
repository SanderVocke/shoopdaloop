# Application session protocol

With `--no-default-features`, messages, validation, codecs and streaming use
`no_std` with `alloc`. The default `std` feature adds the `local` channel adapter.
The wire format is the same in both builds.

The app and RT engine protocols serve different connections. This crate defines
portable host/session/client/generation/operation identities, shared commands,
operation acceptance/results, structural snapshots, attachment and explicit exit
messages. It has no engine, Lua runtime, UI, device or concrete carrier dependency.
Production composition and service authority are still pending in the recovery plan.

Typed local delivery uses bounded channels and shares immutable snapshots by Arc;
it does not serialize values. The client pumps at most 64 messages per turn and
queues its Ready acknowledgment before admitting commands. Queues hold 256 messages;
clients bound in-flight operations at 128 and retained results at 256. Submitted
requests remain distinct from service-accepted operations. Results and confirmed
state publication are separate events. Neither attachment nor loss triggers replay.

Remote records have a u16 total length, SAPP discriminator and exact version,
followed by a bounded bincode payload. They fit the same 1280-byte record carriers
as engine traffic, without sharing engine semantics. Snapshot bytes are separate
from command records, bounded at 8 MiB and transferred in chunks of at most 1024
bytes. Transfer identity, chunk order, total length and revision must agree before
the client publishes a projection. Interrupted transfers are discarded.

The shared snapshot deliberately excludes script runtimes, UI drafts, local file
workflows and media reads. Commands bound variable-size data without invoking the
wire codec on local delivery. Platform authentication/discovery and the exclusive
service lease will be enforced by native composition and the session service;
validating an identity in this codec alone does not authenticate or authorize it.

Focused tests: `cargo test -p shoop_app_protocol -p shoop_app_client`.
