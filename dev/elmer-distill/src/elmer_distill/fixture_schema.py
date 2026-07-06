"""JSON-Schema validation of a fixture `world` object (cnz5o Task 9).

The canonical schema is Rust-generated (testserver Task 2). Resolution order for
the schema file:

  1. explicit `schema_path` argument
  2. `TUXLINK_WORLD_SCHEMA` environment variable
  3. the Rust-generated committed schema, if present, at
     `src-tauri/tuxlink-mcp-testserver/tests/fixtures/world.schema.json`
  4. the committed placeholder at `tests/fixtures/schema/world.schema.json`

The Rust schema is a full-*fixture* schema (top-level `id` + `world`); when the
resolved schema describes the fixture wrapper, `properties.world` is extracted so
`validate_world` always validates the `world` object itself.
"""
import json
import os

import jsonschema

_HERE = os.path.dirname(__file__)
# tests/fixtures/schema/world.schema.json (placeholder) relative to the package.
_PLACEHOLDER = os.path.normpath(
    os.path.join(_HERE, "..", "..", "tests", "fixtures", "schema", "world.schema.json")
)
# The Rust-generated committed schema (may not exist until testserver Task 2 lands).
_RUST_SCHEMA = os.path.normpath(
    os.path.join(
        _HERE, "..", "..", "..", "..",
        "src-tauri", "tuxlink-mcp-testserver", "tests", "fixtures", "world.schema.json",
    )
)


def _resolve_schema_path(schema_path=None):
    if schema_path:
        return schema_path
    env = os.environ.get("TUXLINK_WORLD_SCHEMA")
    if env:
        return env
    if os.path.exists(_RUST_SCHEMA):
        return _RUST_SCHEMA
    return _PLACEHOLDER


def _world_subschema(schema):
    """If `schema` describes the full fixture wrapper (top-level id+world), return
    the sub-schema for the `world` object; otherwise return `schema` unchanged."""
    props = schema.get("properties") or {}
    if "world" in props and isinstance(props["world"], dict):
        return props["world"]
    return schema


def load_schema(schema_path=None):
    """Load and return the world JSON Schema as a dict (world sub-schema when the
    resolved file is the full-fixture schema)."""
    path = _resolve_schema_path(schema_path)
    with open(path) as f:
        schema = json.load(f)
    return _world_subschema(schema)


def validate_world(world, schema_path=None):
    """Validate a fixture `world` dict against the schema. Raises
    `jsonschema.ValidationError` on the first violation."""
    schema = load_schema(schema_path)
    jsonschema.validate(instance=world, schema=schema)
