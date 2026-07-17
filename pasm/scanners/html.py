from __future__ import annotations

import re


HTML_ID_RE = re.compile(r"""\bid\s*=\s*["']([A-Za-z_][A-Za-z0-9_\-:]*)["']""")
HTML_SCRIPT_RE = re.compile(r"""<script\b[^>]*\bsrc\s*=\s*["']([^"']+)["'][^>]*>""", re.IGNORECASE)


def scan_html_symbols(text: str) -> list[tuple[str, int]]:
    symbols: list[tuple[str, int]] = []
    for match in HTML_ID_RE.finditer(text):
        line = text.count("\n", 0, match.start()) + 1
        symbols.append((match.group(1), line))
    return symbols


def scan_html_imports(text: str) -> list[tuple[str, str, int]]:
    return [
        ("html-script", match.group(1), text.count("\n", 0, match.start()) + 1)
        for match in HTML_SCRIPT_RE.finditer(text)
    ]
