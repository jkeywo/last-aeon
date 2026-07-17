"""Provider-neutral context bundles and structured semantic-audit reports."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from hashlib import sha256
from pathlib import Path
import json

from pasm.core.model import SpecEntity
from pasm.implementation.observation import observe_entity_implementation, observe_repository


@dataclass(frozen=True)
class AuditFinding:
    id: str
    category: str
    severity: str
    summary: str
    details: str
    locations: tuple[dict[str, object], ...]
    suggested_resolution: str | None = None


@dataclass(frozen=True)
class AuditReport:
    schema_version: int
    audit_kind: str
    repository_revision: str | None
    bundle_sha256: str
    entity_ids: tuple[str, ...]
    findings: tuple[AuditFinding, ...]


AUDIT_KINDS = frozenset({"architecture", "migration", "design-alignment"})


def build_audit_bundle(entity: SpecEntity, workspace_root: Path) -> dict[str, object]:
    """Build the smallest reviewable bundle from declared PASM ownership."""
    observation = observe_entity_implementation(entity, workspace_root)
    inventory = observe_repository(workspace_root)
    files = []
    for file in observation.files:
        absolute = workspace_root / file.path
        files.append({"path": file.path.as_posix(), "content": absolute.read_text(encoding="utf-8", errors="replace")})
    bundle = {
        "schema_version": 1,
        "entity": _json_ready(entity),
        "entity_ids": [entity.id.value],
        "repository_revision": inventory.revision,
        "files": files,
        "instructions": (
            "Review only the supplied PASM declaration and source slices. Return semantic findings "
            "as JSON with id, category, severity, summary, details, and non-empty locations. "
            "Do not restate deterministic PASM validation findings."
        ),
    }
    return {**bundle, "bundle_sha256": _bundle_sha256(bundle)}


def load_audit_report(path: Path, bundle: dict[str, object] | None = None) -> AuditReport:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ValueError("Audit report must be a JSON object.")
    required_metadata = ("schema_version", "audit_kind", "repository_revision", "bundle_sha256", "entity_ids")
    if any(key not in payload for key in required_metadata):
        raise ValueError("Audit report requires schema_version, audit_kind, repository_revision, bundle_sha256, and entity_ids.")
    if payload["schema_version"] != 1 or payload["audit_kind"] not in AUDIT_KINDS:
        raise ValueError("Audit report has an unknown schema version or audit kind.")
    if payload["repository_revision"] is not None and not isinstance(payload["repository_revision"], str):
        raise ValueError("Audit report repository_revision must be a string or null.")
    if not isinstance(payload["bundle_sha256"], str) or len(payload["bundle_sha256"]) != 64:
        raise ValueError("Audit report bundle_sha256 must be a SHA-256 digest.")
    if not _string_ids(payload["entity_ids"]):
        raise ValueError("Audit report entity_ids must be a non-empty list of strings.")
    if bundle is not None:
        expected = bundle.get("bundle_sha256")
        if payload["bundle_sha256"] != expected:
            raise ValueError("Audit report does not match the supplied audit bundle.")
        if payload["repository_revision"] != bundle.get("repository_revision"):
            raise ValueError("Audit report repository revision does not match the supplied audit bundle.")
        if tuple(payload["entity_ids"]) != tuple(bundle.get("entity_ids", ())):
            raise ValueError("Audit report entity_ids do not match the supplied audit bundle.")
    items = payload.get("findings")
    if not isinstance(items, list):
        raise ValueError("Audit report must be an object containing a findings array.")
    findings: list[AuditFinding] = []
    seen: set[tuple[str, str, tuple[tuple[object, ...], ...]]] = set()
    for item in items:
        if not isinstance(item, dict):
            raise ValueError("Each audit finding must be an object.")
        required = ("id", "category", "severity", "summary", "details", "locations")
        if any(not isinstance(item.get(key), str) or not item[key].strip() for key in required[:-1]):
            raise ValueError("Each audit finding requires non-empty id, category, severity, summary, and details.")
        locations = item.get("locations")
        if not isinstance(locations, list) or not locations or any(not isinstance(location, dict) or not isinstance(location.get("path"), str) for location in locations):
            raise ValueError("Each audit finding requires one or more source locations with paths.")
        normalized_locations = tuple(sorted(tuple(sorted(location.items())) for location in locations))
        key = (item["category"], item["summary"], normalized_locations)
        if key in seen:
            continue
        seen.add(key)
        findings.append(AuditFinding(
            id=item["id"], category=item["category"], severity=item["severity"], summary=item["summary"],
            details=item["details"], locations=tuple(locations), suggested_resolution=item.get("suggested_resolution"),
        ))
    return AuditReport(
        schema_version=payload["schema_version"], audit_kind=payload["audit_kind"],
        repository_revision=payload["repository_revision"], bundle_sha256=payload["bundle_sha256"],
        entity_ids=tuple(payload["entity_ids"]), findings=tuple(findings),
    )


def persist_audit_report(report: AuditReport, bundle: dict[str, object], destination: Path) -> Path:
    """Persist a canonical report and its exact reviewed bundle without overwrites."""
    destination.mkdir(parents=True, exist_ok=True)
    bundle_path = destination / "bundles" / f"{report.bundle_sha256}.json"
    bundle_path.parent.mkdir(parents=True, exist_ok=True)
    bundle_encoded = json.dumps(bundle, indent=2, sort_keys=True) + "\n"
    if bundle_path.exists() and bundle_path.read_text(encoding="utf-8") != bundle_encoded:
        raise ValueError(f"Refusing to overwrite a different audit bundle: {bundle_path}")
    bundle_path.write_text(bundle_encoded, encoding="utf-8")
    revision = (report.repository_revision or "unversioned")[:12]
    path = destination / f"{report.audit_kind}-{report.entity_ids[0]}-{revision}.json"
    payload = {
        "schema_version": report.schema_version,
        "audit_kind": report.audit_kind,
        "repository_revision": report.repository_revision,
        "bundle_sha256": report.bundle_sha256,
        "entity_ids": list(report.entity_ids),
        "findings": [_json_ready(finding) for finding in report.findings],
    }
    encoded = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    if path.exists() and path.read_text(encoding="utf-8") != encoded:
        raise ValueError(f"Refusing to overwrite a different audit report: {path}")
    path.write_text(encoded, encoding="utf-8")
    return path


def _bundle_sha256(bundle: dict[str, object]) -> str:
    return sha256(json.dumps(bundle, sort_keys=True, separators=(",", ":")).encode("utf-8")).hexdigest()


def _string_ids(value: object) -> bool:
    return isinstance(value, list) and bool(value) and all(isinstance(item, str) and item for item in value)


def _json_ready(value):
    if hasattr(value, "__dataclass_fields__"):
        return {key: _json_ready(item) for key, item in asdict(value).items()}
    if isinstance(value, Path):
        return value.as_posix()
    if hasattr(value, "value"):
        return value.value
    if isinstance(value, tuple):
        return [_json_ready(item) for item in value]
    if isinstance(value, dict):
        return {key: _json_ready(item) for key, item in value.items()}
    return value
