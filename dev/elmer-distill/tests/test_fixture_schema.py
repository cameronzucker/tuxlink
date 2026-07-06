"""Task 9 — JSON-Schema validation of the fixture `world`.

Validation points at the Rust-generated schema (Task 2) via TUXLINK_WORLD_SCHEMA
or an explicit path; a committed placeholder under tests/fixtures/schema/ is the
fallback for pre-Task-2 local runs. The placeholder requires the non-optional
`modem` + `position` DTOs (the cross-half contract).
"""
import json
import os

import pytest

from elmer_distill.fixture_schema import validate_world, load_schema

FX = os.path.join(os.path.dirname(__file__), "fixtures", "scenarios")


def _world(name):
    return json.load(open(os.path.join(FX, name)))["world"]


def test_valid_world_passes():
    validate_world(_world("grounded-gateways-01.json"))  # must not raise


def test_invalid_world_raises():
    import jsonschema
    with pytest.raises(jsonschema.ValidationError):
        validate_world(_world("invalid-world-01.json"))


def test_load_schema_reads_file():
    schema = load_schema()
    assert isinstance(schema, dict)
    # the placeholder is a schema for the world object with required non-optional DTOs
    assert "modem" in schema.get("required", []) and "position" in schema.get("required", [])


def test_explicit_schema_path_arg(tmp_path):
    p = tmp_path / "s.schema.json"
    p.write_text(json.dumps({"type": "object", "required": ["modem", "position"]}))
    validate_world(_world("grounded-gateways-01.json"), schema_path=str(p))
    import jsonschema
    with pytest.raises(jsonschema.ValidationError):
        validate_world(_world("invalid-world-01.json"), schema_path=str(p))
