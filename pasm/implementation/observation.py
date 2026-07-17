from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
import re
import subprocess
import tomllib

from pasm.core.model import SourceLocation, SpecEntity
from pasm.scanners.html import scan_html_imports, scan_html_symbols
from pasm.scanners.javascript import scan_javascript_imports, scan_javascript_symbols
from pasm.scanners.rust import scan_rust_imports, scan_rust_symbols
from pasm.scanners.toml import scan_toml_imports, scan_toml_symbols


SUPPORTED_SUFFIXES = {".rs", ".js", ".mjs", ".cjs", ".jsx", ".ts", ".tsx", ".html", ".toml"}
IGNORED_DIRECTORIES = {".git", ".venv", "node_modules", "target", "dist", "build", "__pycache__"}


@dataclass(frozen=True)
class ObservedSymbol:
    name: str
    kind: str
    location: SourceLocation


@dataclass(frozen=True)
class ObservedImport:
    kind: str
    target: str
    location: SourceLocation


@dataclass(frozen=True)
class ObservedFile:
    path: Path
    language: str
    symbols: tuple[ObservedSymbol, ...]
    imports: tuple[ObservedImport, ...]
    raw_text: str

    def has_symbol(self, name: str) -> bool:
        return any(symbol.name == name for symbol in self.symbols)

    def find_symbol(self, name: str) -> ObservedSymbol | None:
        return next((symbol for symbol in self.symbols if symbol.name == name), None)

    def contains_text(self, text: str) -> bool:
        return text in self.raw_text


@dataclass(frozen=True)
class ObservedDependency:
    source: Path
    target: Path
    kind: str
    location: SourceLocation


@dataclass(frozen=True)
class CargoPackage:
    name: str
    manifest_path: Path
    dependencies: tuple[str, ...]


@dataclass(frozen=True)
class RepositoryInventory:
    workspace_root: Path
    revision: str | None
    cargo_packages: tuple[CargoPackage, ...]
    files: tuple[ObservedFile, ...]
    dependencies: tuple[ObservedDependency, ...]


def find_repository_symbol_references(
    inventory: RepositoryInventory,
    symbol: str,
) -> tuple[SourceLocation, ...]:
    """Return lexical identifier references outside a symbol's own declaration.

    This is deliberately evidence, not a call graph: macros, comments, dynamic
    dispatch, and generated code cannot be classified safely by this scanner.
    """
    pattern = re.compile(rf"\b{re.escape(symbol)}\b")
    locations: list[SourceLocation] = []
    for observed_file in inventory.files:
        for line_number, line in enumerate(observed_file.raw_text.splitlines(), start=1):
            if pattern.search(line):
                locations.append(SourceLocation(path=observed_file.path, line=line_number, column=1))
    return tuple(locations)


@dataclass(frozen=True)
class ObservedImplementation:
    entity_id: str
    files: tuple[ObservedFile, ...]

    def has_symbol(self, name: str) -> bool:
        return any(file.has_symbol(name) for file in self.files)

    def find_symbol(self, name: str) -> ObservedSymbol | None:
        for file in self.files:
            symbol = file.find_symbol(name)
            if symbol is not None:
                return symbol
        return None

    def contains_text(self, text: str) -> bool:
        return any(file.contains_text(text) for file in self.files)


@lru_cache(maxsize=8)
def observe_repository(workspace_root: Path) -> RepositoryInventory:
    workspace_root = workspace_root.resolve()
    files = tuple(
        observed
        for path in _repository_files(workspace_root)
        if (observed := _scan_file(path, workspace_root)) is not None
    )
    known_paths = {file.path for file in files}
    dependencies = tuple(
        sorted(
            (
                dependency
                for file in files
                for imported in file.imports
                if (
                    dependency := _resolve_dependency(
                        file, imported, workspace_root, known_paths
                    )
                )
                is not None
            ),
            key=lambda edge: (edge.source.as_posix(), edge.target.as_posix(), edge.kind, edge.location.line or 0),
        )
    )
    return RepositoryInventory(
        workspace_root=workspace_root,
        revision=_git_revision(workspace_root),
        cargo_packages=_observe_cargo_packages(workspace_root),
        files=files,
        dependencies=dependencies,
    )


def observe_entity_implementation(
    entity: SpecEntity,
    workspace_root: Path,
) -> ObservedImplementation:
    implementation = entity.implementation
    if implementation is None:
        return ObservedImplementation(entity_id=entity.id.value, files=())

    observed_files: list[ObservedFile] = []
    seen: set[Path] = set()
    for declared_path in implementation.paths + implementation.legacy_paths + implementation.target_paths:
        absolute_path = (workspace_root / declared_path).resolve()
        if not absolute_path.exists():
            continue
        for file_path in _expand_scan_targets(absolute_path):
            normalized = file_path.resolve()
            if normalized in seen:
                continue
            seen.add(normalized)
            observed = _scan_file(normalized, workspace_root)
            if observed is not None:
                observed_files.append(observed)

    return ObservedImplementation(entity_id=entity.id.value, files=tuple(observed_files))


def _repository_files(workspace_root: Path) -> list[Path]:
    return sorted(
        (
            path
            for path in workspace_root.rglob("*")
            if path.is_file()
            and path.suffix.lower() in SUPPORTED_SUFFIXES
            and not any(part in IGNORED_DIRECTORIES for part in path.relative_to(workspace_root).parts)
        ),
        key=lambda path: path.as_posix(),
    )


def _expand_scan_targets(path: Path) -> list[Path]:
    if path.is_file():
        return [path]
    if not path.is_dir():
        return []
    return sorted(
        (
            file_path
            for file_path in path.rglob("*")
            if file_path.is_file() and file_path.suffix.lower() in SUPPORTED_SUFFIXES
        ),
        key=lambda file_path: file_path.as_posix(),
    )


def _scan_file(path: Path, workspace_root: Path) -> ObservedFile | None:
    suffix = path.suffix.lower()
    language_map = {
        ".rs": ("rust", scan_rust_symbols, scan_rust_imports),
        ".js": ("javascript", scan_javascript_symbols, scan_javascript_imports),
        ".mjs": ("javascript", scan_javascript_symbols, scan_javascript_imports),
        ".cjs": ("javascript", scan_javascript_symbols, scan_javascript_imports),
        ".jsx": ("javascript", scan_javascript_symbols, scan_javascript_imports),
        ".ts": ("typescript", scan_javascript_symbols, scan_javascript_imports),
        ".tsx": ("typescript", scan_javascript_symbols, scan_javascript_imports),
        ".html": ("html", scan_html_symbols, scan_html_imports),
        ".toml": ("toml", scan_toml_symbols, scan_toml_imports),
    }
    entry = language_map.get(suffix)
    if entry is None:
        return None

    language, symbol_scanner, import_scanner = entry
    raw_text = path.read_text(encoding="utf-8", errors="replace")
    relative_path = path.relative_to(workspace_root)
    symbols = tuple(
        ObservedSymbol(
            name=name,
            kind=language,
            location=SourceLocation(path=relative_path, line=line, column=1),
        )
        for name, line in symbol_scanner(raw_text)
    )
    imports = tuple(
        ObservedImport(
            kind=kind,
            target=target,
            location=SourceLocation(path=relative_path, line=line, column=1),
        )
        for kind, target, line in import_scanner(raw_text)
    )
    return ObservedFile(
        path=relative_path,
        language=language,
        symbols=symbols,
        imports=imports,
        raw_text=raw_text,
    )


def _resolve_dependency(
    source: ObservedFile,
    imported: ObservedImport,
    workspace_root: Path,
    known_paths: set[Path],
) -> ObservedDependency | None:
    target = _resolve_import_target(source, imported, workspace_root)
    if target is None or target not in known_paths:
        return None
    return ObservedDependency(
        source=source.path,
        target=target,
        kind=imported.kind,
        location=imported.location,
    )


def _resolve_import_target(
    source: ObservedFile,
    imported: ObservedImport,
    workspace_root: Path,
) -> Path | None:
    source_path = workspace_root / source.path
    if imported.kind == "rust-mod":
        return _first_existing_module(source_path.parent, imported.target, workspace_root)
    if imported.kind == "rust-use":
        return _resolve_rust_use(source_path, imported.target, workspace_root)
    if imported.kind in {"javascript-import", "html-script", "toml-path"}:
        return _resolve_web_import(source_path, imported.target, workspace_root)
    return None


def _resolve_rust_use(source_path: Path, target: str, workspace_root: Path) -> Path | None:
    if target.startswith("crate::"):
        src_root = next((parent for parent in source_path.parents if parent.name == "src"), None)
        if src_root is None:
            return None
        segments = target.split("::")[1:]
        for length in range(len(segments), 0, -1):
            candidate = _first_existing_module(src_root, Path(*segments[:length]).as_posix(), workspace_root)
            if candidate is not None:
                return candidate
    return None


def _resolve_web_import(source_path: Path, target: str, workspace_root: Path) -> Path | None:
    if target.startswith(("http://", "https://", "//", "#")) or not target:
        return None
    workspace_relative = target.startswith("/") or target.startswith("assets/")
    candidate = workspace_root / target.lstrip("/") if workspace_relative else source_path.parent / target
    candidate = candidate.resolve()
    if not candidate.is_relative_to(workspace_root):
        return None
    candidates = [candidate]
    if not candidate.suffix:
        candidates.extend(candidate.with_suffix(suffix) for suffix in (".js", ".mjs", ".cjs", ".ts", ".tsx"))
        candidates.extend(candidate / f"index{suffix}" for suffix in (".js", ".ts"))
    for resolved in candidates:
        if resolved.is_file():
            return resolved.relative_to(workspace_root)
    return None


def _first_existing_module(base: Path, module: str, workspace_root: Path) -> Path | None:
    relative_module = Path(module)
    for candidate in (base / relative_module.with_suffix(".rs"), base / relative_module / "mod.rs"):
        if candidate.is_file() and candidate.is_relative_to(workspace_root):
            return candidate.relative_to(workspace_root)
    return None


def _observe_cargo_packages(workspace_root: Path) -> tuple[CargoPackage, ...]:
    manifests = sorted(
        (
            path
            for path in workspace_root.rglob("Cargo.toml")
            if not any(part in IGNORED_DIRECTORIES for part in path.relative_to(workspace_root).parts)
        ),
        key=lambda path: path.as_posix(),
    )
    packages: list[CargoPackage] = []
    for manifest in manifests:
        try:
            payload = tomllib.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError):
            continue
        package = payload.get("package")
        if not isinstance(package, dict) or not isinstance(package.get("name"), str):
            continue
        dependencies: set[str] = set()
        for key in ("dependencies", "dev-dependencies", "build-dependencies"):
            section = payload.get(key, {})
            if isinstance(section, dict):
                dependencies.update(str(name) for name in section)
        packages.append(
            CargoPackage(
                name=package["name"],
                manifest_path=manifest.relative_to(workspace_root),
                dependencies=tuple(sorted(dependencies)),
            )
        )
    return tuple(packages)


def _git_revision(workspace_root: Path) -> str | None:
    try:
        result = subprocess.run(
            ["git", "-C", str(workspace_root), "rev-parse", "HEAD"],
            capture_output=True,
            check=False,
            text=True,
            timeout=5,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout.strip() if result.returncode == 0 else None
