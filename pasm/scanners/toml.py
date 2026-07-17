from __future__ import annotations

import re


TOML_TABLE_RE = re.compile(r"^\s*\[\[?([^\]]+)\]\]?\s*$", re.MULTILINE)
TOML_PATH_ASSIGNMENT_RE = re.compile(
    r"^\s*(?:template_path|path|extra_worlds|asteroid_type_paths|cosmetic_type_paths)\s*=\s*(.+)$",
    re.MULTILINE,
)
TOML_STRING_RE = re.compile(r'''["']([^"']+)["']''')


def scan_toml_symbols(text: str) -> list[tuple[str, int]]:
    """Expose declared table names as lightweight authored-content symbols."""
    return [
        (match.group(1).strip(), text.count("\n", 0, match.start()) + 1)
        for match in TOML_TABLE_RE.finditer(text)
    ]


def scan_toml_imports(text: str) -> list[tuple[str, str, int]]:
    """Find source-located authored file references without interpreting TOML.

    The narrow key set covers common template and world-layer references. Other
    string values stay data rather than becoming speculative
    dependencies.
    """
    imports: list[tuple[str, str, int]] = []
    for assignment in TOML_PATH_ASSIGNMENT_RE.finditer(text):
        line = text.count("\n", 0, assignment.start()) + 1
        imports.extend(
            ("toml-path", value, line)
            for value in TOML_STRING_RE.findall(assignment.group(1))
        )
    return imports
