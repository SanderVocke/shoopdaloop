from jsonschema import validate, ValidationError
import json
import os

scriptdir = os.path.dirname(os.path.realpath(__file__))

def validate_subtree(t):
    if type(t) is dict:
        for key,value in t.items():
            validate_subtree(value)
        if 'schema' in t:
            validate_single_object(t)
        return
    if type(t) is list:
        for v in t:
            validate_subtree(v)

def validate_single_object(obj):
    if type(obj) is not dict:
        raise ValidationError("Cannot validate a non-dict object.")
    if not "schema" in obj:
        raise ValidationError("no schema set on session (sub-)object.")
    schema_str = obj['schema']
    schema_name = schema_str.split('.')[0]
    schema_version = schema_str.split('.')[1]
    schema_filename = '{}/{}/{}.{}.json'.format(
        scriptdir,
        schema_name,
        schema_name,
        schema_version
    )
    if not os.path.isfile(schema_filename):
        raise ValidationError("schema file not found: {}".format(schema_filename))
    schema = None
    with open(schema_filename, 'r') as f:
        schema = json.load(f)
    validate(instance=obj, schema=schema)

def validate_session_object(obj, schemaname):
    if type(obj) is not dict:
        raise ValidationError("Cannot validate a non-dict object: {}".format(type(obj)))
    if not "schema" in obj:
        raise ValidationError("no schema set on session object.")
    if obj["schema"] != schemaname:
        raise ValidationError("Expected session object to have schema {}, but it has {}".format(schemaname, obj["schema"]))
    validate_subtree(obj)