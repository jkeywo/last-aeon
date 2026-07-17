from __future__ import annotations

import re


RUST_SYMBOL_RE = re.compile(
    r"^\s*(?:pub\s+)?(?:(?:async\s+)?fn|struct|enum|trait|type|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
RUST_USE_RE = re.compile(r"^\s*use\s+((?:crate|self|super)::[A-Za-z_][A-Za-z0-9_:]*)", re.MULTILINE)
RUST_MOD_RE = re.compile(r"^\s*(?:pub\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", re.MULTILINE)


def scan_rust_symbols(text: str) -> list[tuple[str, int]]:
    return _scan_symbols(text, RUST_SYMBOL_RE)


def scan_rust_imports(text: str) -> list[tuple[str, str, int]]:
    imports = _scan_imports(text, RUST_USE_RE, "rust-use")
    imports.extend(_scan_imports(text, RUST_MOD_RE, "rust-mod"))
    return imports


def _scan_symbols(text: str, pattern: re.Pattern[str]) -> list[tuple[str, int]]:
    symbols: list[tuple[str, int]] = []
    for match in pattern.finditer(text):
        line = text.count("\n", 0, match.start()) + 1
        symbols.append((match.group(1), line))
    return symbols


def _scan_imports(text: str, pattern: re.Pattern[str], kind: str) -> list[tuple[str, str, int]]:
    return [
        (kind, match.group(1), text.count("\n", 0, match.start()) + 1)
        for match in pattern.finditer(text)
    ]
