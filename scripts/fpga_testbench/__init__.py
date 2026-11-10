"""Schema-checked architectural values for Rust and future cocotb drivers."""

from functools import lru_cache
import json
from pathlib import Path
import subprocess

from jsonschema import Draft202012Validator, validators

SCHEMA_PATH = Path(__file__).with_name("schema.json")


@lru_cache(maxsize=1)
def schema():
    return json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))


@lru_cache(maxsize=1)
def _validator_class():
    # JSON Schema permits 1.0 as an integer; Rust's integer deserializer does not.
    checker = Draft202012Validator.TYPE_CHECKER.redefine(
        "integer", lambda checker, value: type(value) is int
    ).redefine(
        "string", lambda checker, value: isinstance(value, str)
        and not any(0xD800 <= ord(char) <= 0xDFFF for char in value)
    )
    return validators.extend(Draft202012Validator, type_checker=checker)


@lru_cache(maxsize=1)
def _document_validator():
    definition = schema()
    Draft202012Validator.check_schema(definition)
    return _validator_class()(definition)


def validate(document):
    _document_validator().validate(document)


def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON field: {key}")
        result[key] = value
    return result


def _reject_constant(value):
    raise ValueError(f"non-finite JSON number: {value}")


def loads(text):
    document = json.loads(
        text, object_pairs_hook=_object, parse_constant=_reject_constant
    )
    validate(document)
    return document


def dumps(document):
    validate(document)
    return json.dumps(document, indent=2, sort_keys=True, allow_nan=False) + "\n"


def load(path):
    return loads(Path(path).read_text(encoding="utf-8"))


def dump(document, path):
    Path(path).write_text(dumps(document), encoding="utf-8")


def run_reference(command, document, *, executable):
    """Call the Rust tool once per document, never once per simulation cycle."""
    if command not in {"roundtrip", "compile", "trace", "check"}:
        raise ValueError(f"unknown reference command: {command}")
    result = subprocess.run(
        [str(executable), command], input=dumps(document), text=True,
        capture_output=True, check=True,
    )
    return loads(result.stdout)


def assert_result_matches(expected, observed):
    """Compare a completed quantum after a driver collects accepted FIFO jobs."""
    definition = {"$ref": "#/$defs/StateResult", "$defs": schema()["$defs"]}
    validator = _validator_class()(definition)
    validator.validate(expected)
    validator.validate(observed)
    if expected != observed:
        raise AssertionError(
            "state processor quantum differs\n"
            f"expected: {json.dumps(expected, sort_keys=True)}\n"
            f"observed: {json.dumps(observed, sort_keys=True)}"
        )


def assert_audio_result_matches(expected, observed):
    """Compare samples, buffer readiness, state, FIFO effects, and result exactly."""
    definition = {"$ref": "#/$defs/AudioResult", "$defs": schema()["$defs"]}
    validator = _validator_class()(definition)
    validator.validate(expected)
    validator.validate(observed)
    if expected != observed:
        raise AssertionError(
            "audio pipeline quantum differs\n"
            f"expected: {json.dumps(expected, sort_keys=True)}\n"
            f"observed: {json.dumps(observed, sort_keys=True)}"
        )


def assert_transition_result_matches(expected, observed):
    """Compare boundary decisions, retained requests, journal, and clock advance."""
    definition = {"$ref": "#/$defs/TransitionResult", "$defs": schema()["$defs"]}
    validator = _validator_class()(definition)
    validator.validate(expected)
    validator.validate(observed)
    if expected != observed:
        raise AssertionError(
            "transition processor boundary differs\n"
            f"expected: {json.dumps(expected, sort_keys=True)}\n"
            f"observed: {json.dumps(observed, sort_keys=True)}"
        )
