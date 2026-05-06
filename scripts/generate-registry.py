#!/usr/bin/env python3
"""Generate rMenu rMods registry.json from modules/*.rmod and rpacks/* folders."""

from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any
from urllib.parse import quote

SCHEMA_VERSION = 1
SUPPORTED_API_VERSION = 1
SUPPORTED_MODULE_KIND = "script"
REGISTRY_KIND_RMOD = "rmod"
REGISTRY_KIND_RPACK = "rpack"
REQUIRED_HEADERS = ("name", "version", "api_version", "kind", "capabilities")
REQUIRED_MANIFEST_FIELDS = ("name", "version", "api_version", "kind", "entry", "capabilities")


class RegistryError(Exception):
    """Raised when registry generation cannot continue."""


@dataclass(frozen=True)
class ParsedRmod:
    headers: dict[str, str]
    blocks: dict[str, str]


@dataclass(frozen=True)
class ParsedManifest:
    values: dict[str, Any]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate registry.json from .rmod files and rpack folders."
    )
    parser.add_argument(
        "--modules-dir",
        type=Path,
        default=Path("modules"),
        help="Directory containing .rmod files. Default: modules",
    )
    parser.add_argument(
        "--rpacks-dir",
        type=Path,
        default=Path("rpacks"),
        help="Directory containing rpack module folders. Default: rpacks",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("registry.json"),
        help="Output registry JSON path. Default: registry.json",
    )
    parser.add_argument(
        "--owner",
        default="SynrgStudio",
        help="GitHub owner used to build raw download URLs. Default: SynrgStudio",
    )
    parser.add_argument(
        "--repo",
        default="rmods",
        help="GitHub repository used to build raw download URLs. Default: rmods",
    )
    parser.add_argument(
        "--branch",
        default="main",
        help="Git branch used to build raw download URLs. Default: main",
    )
    parser.add_argument(
        "--download-base-url",
        default=None,
        help=(
            "Optional base URL for .rmod downloads. When omitted, uses "
            "https://raw.githubusercontent.com/<owner>/<repo>/<branch>/modules"
        ),
    )
    parser.add_argument(
        "--rpack-base-url",
        default=None,
        help=(
            "Optional base URL for rpack folders. When omitted, uses "
            "https://raw.githubusercontent.com/<owner>/<repo>/<branch>/rpacks"
        ),
    )
    return parser.parse_args()


def parse_rmod(path: Path) -> ParsedRmod:
    try:
        content = path.read_text(encoding="utf-8-sig")
    except UnicodeDecodeError as error:
        raise RegistryError(f"{path}: .rmod must be UTF-8 text: {error}") from error

    lines = content.splitlines()
    if not lines or lines[0].strip() != "#!rmod/v1":
        raise RegistryError(f"{path}: invalid magic, expected '#!rmod/v1'")

    headers: dict[str, str] = {}
    body_start = None
    for index, line in enumerate(lines[1:], start=1):
        stripped = line.strip()
        if not stripped:
            body_start = index + 1
            break
        if ":" not in stripped:
            raise RegistryError(f"{path}: malformed header line: {stripped!r}")
        key, value = stripped.split(":", 1)
        key = key.strip()
        value = value.strip()
        if not key:
            raise RegistryError(f"{path}: empty header key")
        if key in headers:
            raise RegistryError(f"{path}: duplicate header: {key}")
        headers[key] = value

    if body_start is None:
        body_start = len(lines)

    validate_module_metadata(path, headers)

    blocks = parse_blocks(path, lines[body_start:])
    if "module.js" not in blocks:
        raise RegistryError(f"{path}: missing required block: module.js")

    if "config.json" in blocks:
        try:
            json.loads(blocks["config.json"])
        except json.JSONDecodeError as error:
            raise RegistryError(f"{path}: config.json is not valid JSON: {error}") from error

    return ParsedRmod(headers=headers, blocks=blocks)


def parse_blocks(path: Path, lines: list[str]) -> dict[str, str]:
    blocks: dict[str, str] = {}
    current_name: str | None = None
    current_lines: list[str] = []

    for line in lines:
        stripped = line.strip()
        is_delimiter = (
            stripped.startswith("---") and stripped.endswith("---") and len(stripped) > 6
        )
        if is_delimiter:
            if current_name is not None:
                blocks[current_name] = "\n".join(current_lines).rstrip("\n")
            current_name = stripped.removeprefix("---").removesuffix("---").strip()
            if not current_name:
                raise RegistryError(f"{path}: empty block name")
            if current_name in blocks:
                raise RegistryError(f"{path}: duplicate block: {current_name}")
            current_lines = []
            continue

        if current_name is not None:
            current_lines.append(line)

    if current_name is not None:
        blocks[current_name] = "\n".join(current_lines).rstrip("\n")

    return blocks


def parse_manifest(path: Path) -> ParsedManifest:
    values: dict[str, Any] = {}
    current_section = ""
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("[") and stripped.endswith("]"):
            current_section = stripped.removeprefix("[").removesuffix("]").strip()
            continue
        if "=" not in stripped:
            raise RegistryError(f"{path}: malformed manifest line: {stripped!r}")
        key, raw_value = [part.strip() for part in stripped.split("=", 1)]
        if current_section:
            key = f"{current_section}.{key}"
        values[key] = parse_toml_value(raw_value)

    for required in REQUIRED_MANIFEST_FIELDS:
        if required not in values or values[required] in ("", []):
            raise RegistryError(f"{path}: missing required manifest field: {required}")

    validate_module_metadata(path, {key: manifest_string(values[key]) for key in REQUIRED_HEADERS})
    entry = str(values["entry"])
    entry_path = path.parent / entry
    if not is_safe_relative_path(entry) or not entry_path.exists() or not entry_path.is_file():
        raise RegistryError(f"{path}: invalid or missing entry file: {entry}")

    config_file = values.get("config.file")
    if config_file:
        config_path = path.parent / str(config_file)
        if not is_safe_relative_path(str(config_file)):
            raise RegistryError(f"{path}: invalid config file path: {config_file}")
        try:
            json.loads(config_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            raise RegistryError(f"{config_path}: config JSON is invalid: {error}") from error

    return ParsedManifest(values=values)


def parse_toml_value(raw: str) -> Any:
    raw = raw.strip()
    if raw.startswith('"') and raw.endswith('"'):
        return raw[1:-1]
    if raw in ("true", "false"):
        return raw == "true"
    if raw.startswith("[") and raw.endswith("]"):
        inner = raw[1:-1].strip()
        if not inner:
            return []
        return [parse_toml_value(part.strip()) for part in inner.split(",")]
    try:
        return int(raw)
    except ValueError:
        return raw


def manifest_string(value: Any) -> str:
    if isinstance(value, list):
        return ",".join(str(part) for part in value)
    return str(value)


def validate_module_metadata(path: Path, values: dict[str, str]) -> None:
    for required in REQUIRED_HEADERS:
        if not values.get(required):
            raise RegistryError(f"{path}: missing required metadata: {required}")

    try:
        api_version = int(values["api_version"])
    except ValueError as error:
        raise RegistryError(
            f"{path}: api_version must be numeric: {values['api_version']!r}"
        ) from error
    if api_version != SUPPORTED_API_VERSION:
        raise RegistryError(
            f"{path}: unsupported api_version {api_version}, expected {SUPPORTED_API_VERSION}"
        )

    if values["kind"] != SUPPORTED_MODULE_KIND:
        raise RegistryError(
            f"{path}: unsupported module kind {values['kind']!r}, expected {SUPPORTED_MODULE_KIND!r}"
        )

    capabilities = [part.strip() for part in values["capabilities"].split(",")]
    if not any(capabilities):
        raise RegistryError(f"{path}: capabilities must contain at least one value")


def sha256_hex(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_rpack(files: list[dict[str, Any]]) -> str:
    digest = hashlib.sha256()
    for file in sorted(files, key=lambda item: item["path"]):
        digest.update(file["path"].encode("utf-8"))
        digest.update(b"\0")
        digest.update(file["sha256"].encode("ascii"))
        digest.update(b"\0")
        digest.update(str(file["size"]).encode("ascii"))
        digest.update(b"\n")
    return digest.hexdigest()


def split_csv(value: str | None) -> list[str]:
    if not value:
        return []
    return [part.strip() for part in value.split(",") if part.strip()]


def download_url(base_url: str, filename: str) -> str:
    return f"{base_url.rstrip('/')}/{quote(filename)}"


def rpack_base_url(base_url: str, module_id: str) -> str:
    return f"{base_url.rstrip('/')}/{quote(module_id)}"


def relative_posix_path(root: Path, path: Path) -> str:
    relative = path.relative_to(root).as_posix()
    if not is_safe_relative_path(relative):
        raise RegistryError(f"{path}: unsafe relative path: {relative}")
    return relative


def is_safe_relative_path(path: str) -> bool:
    if not path or path.startswith("/") or "\x00" in path:
        return False
    pure = PurePosixPath(path.replace("\\", "/"))
    return not pure.is_absolute() and all(part not in ("", ".", "..") for part in pure.parts)


def build_module_record(path: Path, parsed: ParsedRmod, base_url: str) -> dict[str, Any]:
    headers = parsed.headers
    module_id = headers["name"]
    description = headers.get("description", "")
    record: dict[str, Any] = {
        "id": module_id,
        "name": headers.get("display_name", module_id),
        "version": headers["version"],
        "description": description,
        "kind": REGISTRY_KIND_RMOD,
        "download_url": download_url(base_url, path.name),
        "sha256": sha256_hex(path),
        "size": path.stat().st_size,
        "tags": split_csv(headers.get("tags")),
    }
    if requires_rmenu := headers.get("requires_rmenu"):
        record["requires_rmenu"] = requires_rmenu
    return record


def build_rpack_record(path: Path, parsed: ParsedManifest, base_url: str) -> dict[str, Any]:
    values = parsed.values
    module_id = str(values["name"])
    files: list[dict[str, Any]] = []
    for file_path in sorted((entry for entry in path.rglob("*") if entry.is_file()), key=lambda p: p.as_posix().lower()):
        relative = relative_posix_path(path, file_path)
        files.append({
            "path": relative,
            "sha256": sha256_hex(file_path),
            "size": file_path.stat().st_size,
        })

    record: dict[str, Any] = {
        "id": module_id,
        "name": str(values.get("display_name", module_id)),
        "version": str(values["version"]),
        "description": str(values.get("description", "")),
        "kind": REGISTRY_KIND_RPACK,
        "base_url": rpack_base_url(base_url, module_id),
        "sha256": sha256_rpack(files),
        "size": sum(file["size"] for file in files),
        "files": files,
        "tags": split_csv(str(values.get("tags", ""))),
    }
    if requires_rmenu := values.get("requires_rmenu"):
        record["requires_rmenu"] = str(requires_rmenu)
    return record


def validate_duplicate_ids(records: list[dict[str, Any]]) -> None:
    seen: dict[str, str] = {}
    for record in records:
        module_id = record["id"].lower()
        source = record.get("download_url") or record.get("base_url") or "unknown"
        if module_id in seen:
            raise RegistryError(
                f"duplicate module id {record['id']!r} from {source} and {seen[module_id]}"
            )
        seen[module_id] = source


def generate_registry(modules_dir: Path, rpacks_dir: Path, modules_base_url: str, rpacks_base_url: str) -> dict[str, Any]:
    records: list[dict[str, Any]] = []

    if modules_dir.exists():
        if not modules_dir.is_dir():
            raise RegistryError(f"modules path is not a directory: {modules_dir}")
        module_paths = sorted(modules_dir.glob("*.rmod"), key=lambda path: path.name.lower())
        records.extend(build_module_record(path, parse_rmod(path), modules_base_url) for path in module_paths)

    if rpacks_dir.exists():
        if not rpacks_dir.is_dir():
            raise RegistryError(f"rpacks path is not a directory: {rpacks_dir}")
        rpack_paths = sorted((path for path in rpacks_dir.iterdir() if path.is_dir()), key=lambda path: path.name.lower())
        records.extend(build_rpack_record(path, parse_manifest(path / "module.toml"), rpacks_base_url) for path in rpack_paths)

    if not records:
        raise RegistryError("no modules or rpacks found")

    records.sort(key=lambda record: record["id"].lower())
    validate_duplicate_ids(records)

    return {
        "schema": SCHEMA_VERSION,
        "generated_at": datetime.now(timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
        "modules": records,
    }


def write_registry(path: Path, registry: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(registry, indent=2, ensure_ascii=False, sort_keys=False) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    args = parse_args()
    modules_base_url = args.download_base_url or (
        f"https://raw.githubusercontent.com/{args.owner}/{args.repo}/{args.branch}/modules"
    )
    rpacks_base_url = args.rpack_base_url or (
        f"https://raw.githubusercontent.com/{args.owner}/{args.repo}/{args.branch}/rpacks"
    )

    try:
        registry = generate_registry(args.modules_dir, args.rpacks_dir, modules_base_url, rpacks_base_url)
        write_registry(args.output, registry)
    except RegistryError as error:
        print(f"error: {error}")
        return 1

    print(f"wrote {args.output} with {len(registry['modules'])} module(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
