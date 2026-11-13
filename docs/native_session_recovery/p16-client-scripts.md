# P16 contract: client-owned session scripts and resources

Status: implementation and local validation complete; final CI and review remain OPEN.
The ownership revision was agreed on 2026-11-12.
This contract supersedes the earlier proposal to store script data in the service.
See the [investigation](p16-investigation.md) and
[implementation plan](../../PLAN_P16_CLIENT_SCRIPT_PERSISTENCE.md).

## Ownership and scope

The service owns the live musical session and admitted musical operations. The
client owns script source, embedded resources, session inclusion, saved preferences,
VMs, timers, dialogs and connections to its own MIDI devices. A `.shoop` file is a
container for both musical state and client-owned additions; it is not just a
serialization of service state. Here, **session-owned script** means a script
included in that client's project file, rather than a machine-local script.

P16 restores include-in-session, Session to Ephemeral/run-once conversion, removal,
exact source export, save/store/load and New Session handling. Capture the current
source and the reference-supported Markdown/PNG companions; reuse embedded bundles
after originals disappear. General Lua dependency discovery and writing an entire
bundle back to a filesystem directory are outside this feature.

No script source, resource bundle or execution preference is added to backend
catalogs, attachment snapshots or musical persistence transfers. There is no
backend script execution or P16 resource extension to the RT protocol. Client-local
worker messages may carry these additions for archive processing.

## Save and store

The client takes an immutable copy of its included script definitions, preferences
and resource bundles for the save task, and combines it with the existing service's
immutable musical capture. Later client edits must not change that task's file.
Fence the pair against session replacement; never combine scripts from one local
project with a capture of a different musical session. Document IDs and resource
owner references must remain valid in the assembled archive.

Include exact source/resource bytes and saved enabled preferences, but never VM
state, callbacks, timers, dialogs, device handles or machine filesystem paths.
Native output completion and browser download-handoff semantics remain unchanged.

## Load and New Session

1. Decode and validate the complete file on the client, including resource paths,
   ownership, hashes, counts and sizes. Check supported Lua syntax without executing
   code. Preserve structurally valid supported-format script bundles that cannot
   run because of unavailable scripting/API/device support; keep them inactive and
   exportable. Unsupported archive formats and malformed data refuse explicitly,
   rather than being stripped.
2. Prepare the replacement client script records and resource providers before
   musical commit, reserving the resources needed for local installation. Keep
   existing scripts and resources intact during validation and conversion consent.
   Send only the musical portion through the existing staged service load.
3. Prevent callbacks and queued old-session script commands from affecting the
   replacement while commit is pending. Tie the prepared client data to that exact
   load operation and target session. Backend refusal or cancellation before
   admission discards the candidate and preserves the previous local script data.
4. Once backend success is confirmed, replace only session-owned client records
   with the prepared records. Loaded scripts start **stopped**, irrespective of
   stored enabled preferences. Preserve machine-local User/Bundled/Ephemeral
   records. An empty script section or successful New Session clears only the
   client's session-owned records. Do not report full load completion until local
   installation has also completed.

The service's musical replacement stays atomic. There is no cross-process atomic
transaction covering both backend state and client memory. After admission, a
cancel or disconnect cannot promise backend rollback. A surviving client retains
its pending local candidate and reconciles the same operation without replaying
it; it must not attach old or unconfirmed scripts to a different session. If the
outcome cannot be established, report uncertainty and keep affected scripts
inactive. Losing the client process can lose the pending local data even if the
backend committed the music.

## Conversion and execution

Collect and validate all supported companions before changing local ownership.
Native scans run outside the UI pump with bounded concurrency and cancellation;
completion checks source/resource generation and project identity. A failed or
stale scan changes neither source nor ownership. Conversion stops the affected VM;
Session to Ephemeral retains its in-memory bundle, so conversion back works without
the original directory. Removal releases the relevant providers and callbacks.

Browser workflows reuse embedded/catalog resources or explicit source-only inputs;
they cannot claim to capture an inaccessible native directory. Missing required
companions produce a useful error. Loading, reconnecting, catalog refresh and
recovery never grant permission to start a script automatically. An explicit start
uses the current client's devices and permissions. Driver switching preserves
client script data and follows existing VM lifecycle rules; it does not reload
scripts from the backend.

## Recovery boundary

Backend recovery restores music and accepted musical operations. A different
client does not inherit script definitions, resources or device configuration.
Its save contains the music and its own included scripts, not the previous
client's additions. Recovering those additions requires the original session file
or retained client memory. Durable client crash recovery and backend attachment
storage are separate future features, not P16 completion requirements.

## Bounds and completion

Retain explicit bounds for resource files, bundles, archive metadata/entries,
expansion, total encoded bytes, worker messages and concurrent jobs. Charge
script/resource bytes as client archive memory alongside musical codec scratch;
they do not consume service media-transfer objects or RT media pages. No production
path may fall back to the reference codec's large default decode limits.

P16 closes only with production native/browser evidence for conversion, exact
round trips after source removal, failed/cancelled loads, stopped installation,
preservation of machine scripts, immutable saves, and recovery without script
transfer/replay. Reference tests remain behavioral evidence; their automatic-start
expectation is intentionally replaced by this explicit-start contract.
