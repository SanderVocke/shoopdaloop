Here is a comprehensive analysis of the **port inheritance tree** and **dependencies** in the C++ backend.

---

## Inheritance Tree

The port hierarchy uses **virtual inheritance** heavily to allow diamond inheritance (e.g. a port that is both an `AudioPort` and a `DummyPort`).

### 1. Core Abstract Bases
```
PortInterface                          (pure virtual, all ports)
├── AudioPort<SampleT>                 (virtual PortInterface)
│   └── (templated on SampleT; audio_sample_t (=float) and int are instantiated)
├── MidiPort                           (virtual PortInterface)
│   ├── MidiBufferingInputPort         (MidiPort + MidiReadableBufferInterface)
│   └── MidiSortingReadWritePort       (MidiPort)
└── DummyPort                          (virtual PortInterface)
```

### 2. Concrete Audio Ports
```
DummyAudioPort
├── virtual AudioPort<audio_sample_t>
├── DummyPort
├── ModuleLoggingEnabled<"Backend.DummyAudioPort">
└── WithCommandQueue

InternalAudioPort<SampleT>
└── AudioPort<SampleT>

GenericJackAudioPort<API>
├── virtual AudioPort<jack_default_audio_sample_t>
└── virtual GenericJackPort<API>
    └── virtual PortInterface
    └── ModuleLoggingEnabled<"Backend.JackPort">

Instantiations:
  JackAudioPort      = GenericJackAudioPort<JackApi>
  JackTestAudioPort  = GenericJackAudioPort<JackTestApi>
```

### 3. Concrete MIDI Ports
```
DummyMidiPort
├── virtual MidiPort
├── DummyPort
├── MidiReadableBufferInterface
├── MidiWriteableBufferInterface
├── WithCommandQueue
└── ModuleLoggingEnabled<"Backend.DummyMidiPort">

GenericJackMidiInputPort<API>
├── virtual MidiBufferingInputPort  (MidiPort + MidiReadableBufferInterface)
└── GenericJackMidiPort<API>
    └── GenericJackPort<API>        (virtual PortInterface)

GenericJackMidiOutputPort<API>
├── virtual MidiSortingReadWritePort (MidiPort)
└── GenericJackMidiPort<API>
    └── GenericJackPort<API>        (virtual PortInterface)

Instantiations:
  JackMidiInputPort      = GenericJackMidiInputPort<JackApi>
  JackTestMidiInputPort  = GenericJackMidiInputPort<JackTestApi>
  JackMidiOutputPort     = GenericJackMidiOutputPort<JackApi>
  JackTestMidiOutputPort = GenericJackMidiOutputPort<JackTestApi>

InternalLV2MidiOutputPort
├── MidiPort
└── MidiWriteableBufferInterface
```

### 4. Graph Wrappers (not PortInterface subclasses, but *own* a port)
These exist for scheduling in the audio graph. They inherit `HasTwoGraphNodes` (not `PortInterface`).

```
GraphPort                          (HasTwoGraphNodes)
├── GraphAudioPort                 (owns shoop_shared_ptr<AudioPort<audio_sample_t>>)
└── GraphMidiPort                  (owns shoop_shared_ptr<MidiPort>)
```

### 5. Decoupled MIDI Port (wrapper, not inheriting PortInterface)
```
DecoupledMidiPort<TimeType, SizeType>
└── shoop_enable_shared_from_this
    (contains a shoop_shared_ptr<MidiPort> internally)
```

### 6. MIDI Buffer Interfaces (used by MidiPort subclasses)
```
MidiSortableMessageInterface
MidiReadableBufferInterface
MidiWriteableBufferInterface
└── MidiReadWriteBufferInterface  (inherits both above)

Implementors:
- MidiSortingBuffer          (MidiReadWriteBufferInterface)
- MidiBufferingInputPort     (MidiReadableBufferInterface)
- DummyMidiPort              (MidiReadable + MidiWriteable)
- InternalLV2MidiOutputPort  (MidiWriteable)
```

---

## Dependencies of Port Classes on Other C++ Classes

| Port Class | Key Dependencies (other C++ classes / headers) |
|---|---|
| **PortInterface** | `types.h` (C enum types), `<string>`, `<map>`, `<stdint.h>` |
| **AudioPort\<T\>** | `PortInterface`, `BufferQueue<T>`, `BufferPool<T>`, `shoop_shared_ptr.h`, `<atomic>` |
| **MidiPort** | `PortInterface`, `MidiStateTracker`, `MidiBufferInterfaces`, `MidiRingbuffer`, `LoggingBackend`, `shoop_shared_ptr.h`, `<atomic>` |
| **DummyPort** | `AudioMidiDriver` (for `ExternalPortDescriptor`), `PortInterface`, `LoggingEnabled`, `types.h` |
| **DummyAudioPort** | `AudioPort`, `DummyPort`, `AudioMidiDriver`, `WithCommandQueue`, `types.h`, `shoop_shared_ptr.h`, `boost::lockfree::spsc_queue` |
| **DummyMidiPort** | `MidiPort`, `DummyPort`, `AudioMidiDriver`, `WithCommandQueue`, `MidiMessage`, `types.h`, `boost::lockfree::spsc_queue` |
| **InternalAudioPort\<T\>** | `AudioPort<T>`, `shoop_shared_ptr.h`, `<vector>` |
| **GenericJackPort\<API\>** | `jack/types.h`, `JackApi.h` / `JackTestApi.h`, `PortInterface`, `GenericJackAllPorts<API>`, `LoggingEnabled` |
| **GenericJackAudioPort\<API\>** | `AudioPort<jack_default_audio_sample_t>`, `GenericJackPort<API>`, `types.h`, `<atomic>` |
| **GenericJackMidiInputPort\<API\>** | `MidiBufferingInputPort`, `GenericJackMidiPort<API>`, `jack_wrappers.h` |
| **GenericJackMidiOutputPort\<API\>** | `MidiSortingReadWritePort`, `GenericJackMidiPort<API>`, `jack_wrappers.h` |
| **MidiBufferingInputPort** | `MidiPort`, `MidiBufferInterfaces`, `LoggingEnabled` |
| **MidiSortingReadWritePort** | `MidiPort`, `MidiSortingBuffer`, `MidiBufferInterfaces`, `shoop_shared_ptr.h` |
| **InternalLV2MidiOutputPort** | `MidiPort`, `lv2_evbuf.h` (external LV2 lib) |
| **GraphPort** | `BackendSession`, `GraphNode` (incl. `HasTwoGraphNodes`), `PortInterface`, `MidiPort`, `LoggingEnabled`, `shoop_globals.h` |
| **GraphAudioPort** | `GraphPort`, `AudioPort`, `shoop_shared_ptr.h` |
| **GraphMidiPort** | `GraphPort`, `MidiPort`, `shoop_shared_ptr.h` |
| **DecoupledMidiPort** | `MidiPort`, `AudioMidiDriver` (fwd), `LoggingEnabled`, `boost::lockfree::spsc_queue` |

---

## Key Infrastructure Dependencies

Several already-existing Rust dependencies are worth noting because they show where CXX bridges already exist:

- **`BufferPool<T>`** and **`AudioBuffer<T>`** both include **`refilling_pool/src/cxx.rs.h`** — a Rust `refilling_pool` crate already manages audio buffers via CXX.
- **`shoop_shared_ptr.h`** may map to `std::shared_ptr` or a tracked variant.
- **`LoggingEnabled.h`** / **`ModuleLoggingEnabled`** uses `LoggingBackend.h` and `fmt::format_string`.
- **`WithCommandQueue`** depends on `CommandQueue.h` (a lock-free queue).

---

## Summary for Porting Strategy

If you plan to port “from the inside out” with `cxx`, the **cleanest starting points** are:

1. **The buffer infrastructure** (`BufferPool`/`AudioBuffer`) — already bridged to Rust (`refilling_pool`). You could expand this pattern.
2. **The MIDI storage/buffer classes** (`MidiStorage`, `MidiSortingBuffer`, `MidiRingbuffer`) — these are pure data structures with no driver dependencies.
3. **`MidiStateTracker`** — self-contained state machine.
4. **`MidiMessage`** — simple data struct.

The **hardest parts to port last** are the driver-bound leaf classes:
- `GenericJackAudioPort`, `GenericJackMidiInputPort`, `GenericJackMidiOutputPort` (direct JACK API usage)
- `DummyAudioPort`, `DummyMidiPort` (threading, queues, driver mocks)
- `GraphAudioPort` / `GraphMidiPort` / `GraphPort` (tightly coupled to `BackendSession` graph scheduling)

`PortInterface`, `AudioPort`, and `MidiPort` are the **central abstract bases** that C++ will need to keep seeing (via `cxx`) until all subclasses are ported.
