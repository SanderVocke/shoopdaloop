const LoopState = {
    Unknown: -1,
    Off: 0,
    WaitStart: 1,
    Recording: 2,
    WaitStop: 3,
    Playing: 4,
    Overdubbing: 5,
    Multiplying: 6,
    Inserting: 7,
    Replacing: 8,
    Delay: 9,
    Muted: 10,
    Scratching: 11,
    OneShot: 12,
    Substitute: 13,
    Paused: 14,
    // Extended states for FX loops
    RecordingWet: 15,
    PlayingDryLiveWet: 16,
    PlayingDry: 17
}

const LoopStateDesc = {
    [LoopState.Unknown]: "Unknown",
    [LoopState.Off]: "Off",
    [LoopState.WaitStart]: "WaitStart",
    [LoopState.Recording]: "Recording",
    [LoopState.WaitStop]: "WaitStop",
    [LoopState.Playing]: "Playing",
    [LoopState.Overdubbing]: "Overdubbing",
    [LoopState.Multiplying]: "Multiplying",
    [LoopState.Inserting]: "Inserting",
    [LoopState.Replacing]: "Replacing",
    [LoopState.Delay]: "Delay",
    [LoopState.Muted]: "Muted",
    [LoopState.Scratching]: "Scratching",
    [LoopState.OneShot]: "OneShot",
    [LoopState.Substitute]: "Substitute",
    [LoopState.Paused]: "Paused",
    // Extended states for FX loops
    [LoopState.RecordingWet]: "RecordingWet",
    [LoopState.PlayingDryLiveWet]: "PlayingDryLiveWet",
    [LoopState.PlayingDry]: "PlayingDry"
}
