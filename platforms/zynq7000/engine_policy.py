"""Build-time engine resource policy, independent of board and target support."""
import copy
import json

DEFAULT = {
    "tracks": 8,
    "loops": 32,
    "input_history": {
        "seconds": 0,
        "audio_take_pages": 32768,
        "midi_pages": 4096,
        "midi_records": 65536,
        "writers": 8,
    },
}


def load(path=None):
    policy = copy.deepcopy(DEFAULT)
    if path is not None:
        values = json.loads(path.read_text())
        if not isinstance(values, dict) or set(values) - set(policy):
            raise ValueError("Unknown engine policy fields")
        history = values.get("input_history", {})
        if not isinstance(history, dict) or set(history) - set(policy["input_history"]):
            raise ValueError("Unknown input-history policy fields")
        policy.update({key: value for key, value in values.items() if key != "input_history"})
        policy["input_history"].update(history)
    for name in ("tracks", "loops"):
        if type(policy[name]) is not int or not 1 <= policy[name] <= 65535:
            raise ValueError(f"{name} must be a positive u16")
    for name, value in policy["input_history"].items():
        if type(value) is not int or not 0 <= value <= 0xffffffff:
            raise ValueError(f"input_history.{name} must be a u32")
        if name != "seconds" and policy["input_history"]["seconds"] > 0 and value == 0:
            raise ValueError(f"Enabled input history requires positive {name}")
    return policy


def environment(policy):
    return {
        "SHOOP_ENGINE_TRACKS": str(policy["tracks"]),
        "SHOOP_ENGINE_LOOPS": str(policy["loops"]),
        **{"SHOOP_INPUT_HISTORY_" + name.upper(): str(value)
           for name, value in policy["input_history"].items()},
    }
