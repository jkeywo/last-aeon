from __future__ import annotations

import re


JAVASCRIPT_SYMBOL_RE = re.compile(
    r"^\s*(?:export\s+)?(?:(?:async\s+)?function|class)\s+([A-Za-z_][A-Za-z0-9_]*)"
    r"|^\s*export\s+(?:const|let|var)\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
JAVASCRIPT_IMPORT_RE = re.compile(
    r"^\s*(?:import|export)\s+(?:[^\"']*?\s+from\s+)?[\"']([^\"']+)[\"']"
    r"|\bimport\(\s*[\"']([^\"']+)[\"']\s*\)",
    re.MULTILINE,
)


def scan_javascript_symbols(text: str) -> list[tuple[str, int]]:
    return _scan_symbols(text, JAVASCRIPT_SYMBOL_RE)


def scan_javascript_imports(text: str) -> list[tuple[str, str, int]]:
    imports: list[tuple[str, str, int]] = []
    for match in JAVASCRIPT_IMPORT_RE.finditer(text):
        target = match.group(1) or match.group(2)
        imports.append(("javascript-import", target, text.count("\n", 0, match.start()) + 1))
    return imports


def _scan_symbols(text: str, pattern: re.Pattern[str]) -> list[tuple[str, int]]:
    symbols: list[tuple[str, int]] = []
    for match in pattern.finditer(text):
        line = text.count("\n", 0, match.start()) + 1
        name = match.group(1) or match.group(2)
        symbols.append((name, line))
    return symbols
