#!/usr/bin/env python3
"""Generate the config-builder setting catalogue from Islandora Workbench's own source.

Reads ``WorkbenchConfig.get_default_config()`` with :mod:`ast` (never ``exec`` -- that file
imports the world), infers a value *shape* for each key from its default literal, merges the
hand-written overrides on top, and writes ``config_catalog.json``.

The overrides file carries everything Workbench does not encode. Its `settings` section patches
keys that DO have an upstream default (descriptions, enum choices, which strings are really
paths or URLs, which keys are internal and skipped); its `extra` section adds the settings
Workbench recognises but has no default for, so they never appear in get_default_config() --
`task`, `shutdown`, `csv_field_templates` and friends.

Usage::

    python scripts/gen-config-catalog.py --workbench /path/to/islandora_workbench

Exits non-zero if an override names a key that no longer exists upstream. That is the drift
signal: upstream renamed or removed something and the override needs the same edit.
"""

from __future__ import annotations

import argparse
import ast
import datetime as _dt
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
OVERRIDES = REPO / "crates" / "workbench-integration" / "catalog_overrides.json"
OUTPUT = REPO / "crates" / "workbench-integration" / "config_catalog.json"

# Shapes must stay in step with `Shape` in crates/workbench-integration/src/config/catalog.rs.
SHAPES = {
    "boolean",
    "integer",
    "string",
    "delimiter",
    "enum",
    "nullable_enum",
    "file_path",
    "url",
    "list_of_strings",
    "list_of_numbers",
    "list_of_one_key_maps",
    "map",
    "map_of_lists",
    "command_list",
    "template_string",
    "config_ref",
}


class Unresolved:
    """A default the generator could not evaluate (e.g. ``tempfile.gettempdir()``)."""

    def __init__(self, expr: str) -> None:
        self.expr = expr


def _static_methods(cls: ast.ClassDef) -> dict[str, ast.FunctionDef]:
    return {n.name: n for n in cls.body if isinstance(n, ast.FunctionDef)}


def _eval(node: ast.AST, methods: dict[str, ast.FunctionDef]):
    """Literal-eval `node`, additionally resolving the few call forms Workbench uses.

    ``dict()`` / ``list()`` / ``dict({...})`` are containers written the long way, and
    ``self.get_media_types()`` and friends are ``@staticmethod``s that return a literal --
    both are worth following. Anything else becomes `Unresolved` and needs an override.
    """
    try:
        return ast.literal_eval(node)
    except (ValueError, SyntaxError):
        pass

    if isinstance(node, ast.Call):
        func = node.func
        if isinstance(func, ast.Name) and func.id in ("dict", "list", "tuple", "set"):
            if not node.args:
                return {} if func.id == "dict" else []
            return _eval(node.args[0], methods)
        # self.get_media_fields() -> follow the static method's `return <literal>`
        if isinstance(func, ast.Attribute) and not node.args:
            target = methods.get(func.attr)
            if target is not None:
                for stmt in ast.walk(target):
                    if isinstance(stmt, ast.Return) and stmt.value is not None:
                        return _eval(stmt.value, methods)

    return Unresolved(ast.unparse(node))


def infer_shape(value) -> str:
    """Best guess at a value shape from the default alone. Overrides refine it."""
    if isinstance(value, Unresolved) or value is None:
        return "string"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, str):
        return "delimiter" if len(value) == 1 else "string"
    if isinstance(value, dict):
        if value and all(isinstance(v, list) for v in value.values()):
            return "map_of_lists"
        return "map"
    if isinstance(value, list):
        if not value:
            # An empty list is ambiguous; strings are much the commoner case upstream.
            return "list_of_strings"
        first = value[0]
        if isinstance(first, dict):
            if all(len(d) == 1 for d in value) and any(
                isinstance(v, list) for d in value for v in d.values()
            ):
                return "map_of_lists"
            return "list_of_one_key_maps"
        if isinstance(first, bool):
            return "list_of_strings"
        if isinstance(first, int):
            return "list_of_numbers"
        return "list_of_strings"
    return "string"


def infer_group(key: str) -> str:
    """Fall-back section for the add-a-setting palette, by key name.

    Overrides win; this only stops the long tail of `log_*` / `*_media_*` keys from all
    landing in one undifferentiated pile.
    """
    for needle, group in (
        ("log_", "Logging"),
        ("rollback", "Rollback"),
        ("paged_content", "Paged content"),
        ("http_", "HTTP"),
        ("export", "Export"),
        ("csv", "Input"),
        ("media", "Media"),
        ("term", "Taxonomy"),
        ("script", "Scripts"),
        ("template", "Templates"),
    ):
        if needle in key:
            return group
    return "Advanced"


def read_defaults(workbench: Path) -> dict[str, object]:
    source = (workbench / "WorkbenchConfig.py").read_text(encoding="utf-8")
    tree = ast.parse(source)

    cls = next(
        (n for n in ast.walk(tree) if isinstance(n, ast.ClassDef)),
        None,
    )
    if cls is None:
        sys.exit("WorkbenchConfig.py: no class found")
    methods = _static_methods(cls)

    fn = methods.get("get_default_config")
    if fn is None:
        sys.exit("WorkbenchConfig.py: get_default_config() not found")

    ret = next(
        (n for n in ast.walk(fn) if isinstance(n, ast.Return) and n.value is not None),
        None,
    )
    if ret is None or not isinstance(ret.value, ast.Dict):
        sys.exit("get_default_config(): expected `return { ... }` with a dict literal")

    defaults: dict[str, object] = {}
    for key_node, val_node in zip(ret.value.keys, ret.value.values):
        if key_node is None:  # `**spread` -- not used upstream, but do not silently drop it
            sys.exit("get_default_config(): dict unpacking is not supported")
        defaults[ast.literal_eval(key_node)] = _eval(val_node, methods)
    return defaults


def workbench_ref(workbench: Path) -> str:
    try:
        out = subprocess.run(
            ["git", "-C", str(workbench), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        )
        return out.stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--workbench",
        required=True,
        type=Path,
        help="path to an islandora_workbench checkout",
    )
    ap.add_argument("--output", type=Path, default=OUTPUT)
    ap.add_argument("--overrides", type=Path, default=OVERRIDES)
    args = ap.parse_args()

    defaults = read_defaults(args.workbench)
    raw = json.loads(args.overrides.read_text(encoding="utf-8"))
    overrides = raw.get("settings", {})
    # Settings Workbench recognises but has no default for, so they never appear in
    # get_default_config(). Curated by hand; each needs a full entry, not a patch.
    extra = raw.get("extra", {})

    # Drift check: an override for a key upstream no longer has is a silent lie otherwise.
    unknown = sorted(set(overrides) - set(defaults))
    if unknown:
        print(
            "error: overrides name keys that are not in get_default_config():\n  "
            + "\n  ".join(unknown),
            file=sys.stderr,
        )
        return 1

    clash = sorted(set(extra) & set(defaults))
    if clash:
        print(
            "error: `extra` names keys that DO have an upstream default; move them to"
            " `settings`:\n  " + "\n  ".join(clash),
            file=sys.stderr,
        )
        return 1

    settings = []
    unresolved = []
    for key, default in list(defaults.items()) + [(k, v.get("default")) for k, v in extra.items()]:
        ov = overrides.get(key) or extra.get(key, {})
        if ov.get("skip"):
            continue

        if isinstance(default, Unresolved):
            if "default" in ov:
                default = ov["default"]
            else:
                unresolved.append(f"{key} = {default.expr}")
                default = None

        shape = ov.get("shape") or infer_shape(default)
        if shape not in SHAPES:
            print(f"error: {key}: unknown shape {shape!r}", file=sys.stderr)
            return 1

        entry = {
            "key": key,
            "shape": shape,
            "default": ov.get("default", default),
            "description": ov.get("description", ""),
            "group": ov.get("group") or infer_group(key),
            "required": ov.get("required", False),
        }
        for optional in ("choices", "unit", "tokens", "browse", "locked"):
            if optional in ov:
                entry[optional] = ov[optional]
        settings.append(entry)

    if unresolved:
        # Not fatal: these get a null default and still appear in the builder. Worth seeing.
        print(
            "note: defaults that could not be evaluated (add a `default` override to fix):\n  "
            + "\n  ".join(unresolved),
            file=sys.stderr,
        )

    catalog = {
        "workbench_ref": workbench_ref(args.workbench),
        "generated": _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%d"),
        "settings": settings,
    }
    args.output.write_text(
        json.dumps(catalog, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"wrote {args.output.relative_to(REPO)} with {len(settings)} settings")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
