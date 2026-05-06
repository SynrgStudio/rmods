#!/usr/bin/env python3
"""Generate rMenu rMods registry.json from modules/*.rmod."""

from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import quote

SCHEMA_VERSION = 1
SUPPORTED_API_VERSION = 1
SUPPORTED_MODULE_KIND = "script"
REGISTRY_KIND = "rmod"
REQUIRED_HEADERS = ("name", "version", "api_version", "kind", "capabilities")


class RegistryError(Exception):
    """Raised when registry generation cannot continue."""


@dataclass(frozen=True)
class ParsedRmod:
    headers: dict[str, str]
    blocks: dict[str, str]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate registry.json from a directory of .rmod files."
    )
    parser.add_argument(
        "--modules-dir",
        type=Path,
        default=Path("modules"),
        help="Directory containing .rmod files. Default: modules",
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
            "Optional base URL for module downloads. When omitted, uses "
            "https://raw.githubusercontent.com/<owner>/<repo>/<branch>/modules"
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

    for required in REQUIRED_HEADERS:
        if not headers.get(required):
            raise RegistryError(f"{path}: missing required header: {required}")

    try:
        api_version = int(headers["api_version"])
    except ValueError as error:
        raise RegistryError(
            f"{path}: api_version must be numeric: {headers['api_version']!r}"
        ) from error
    if api_version != SUPPORTED_API_VERSION:
        raise RegistryError(
            f"{path}: unsupported api_version {api_version}, expected {SUPPORTED_API_VERSION}"
        )

    if headers["kind"] != SUPPORTED_MODULE_KIND:
        raise RegistryError(
            f"{path}: unsupported module kind {headers['kind']!r}, expected {SUPPORTED_MODULE_KIND!r}"
        )

    capabilities = [part.strip() for part in headers["capabilities"].split(",")]
    if not any(capabilities):
        raise RegistryError(f"{path}: capabilities must contain at least one value")

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


def sha256_hex(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def split_csv(value: str | None) -> list[str]:
    if not value:
        return []
    return [part.strip() for part in value.split(",") if part.strip()]


def download_url(base_url: str, filename: str) -> str:
    return f"{base_url.rstrip('/')}/{quote(filename)}"


def build_module_record(path: Path, parsed: ParsedRmod, base_url: str) -> dict[str, Any]:
    headers = parsed.headers
    module_id = headers["name"]
    description = headers.get("description", "")
    record: dict[str, Any] = {
        "id": module_id,
        "name": headers.get("display_name", module_id),
        "version": headers["version"],
        "description": description,
        "kind": REGISTRY_KIND,
        "download_url": download_url(base_url, path.name),
        "sha256": sha256_hex(path),
        "size": path.stat().st_size,
        "tags": split_csv(headers.get("tags")),
    }
    if requires_rmenu := headers.get("requires_rmenu"):
        record["requires_rmenu"] = requires_rmenu
    return record


def validate_duplicate_ids(records: list[dict[str, Any]]) -> None:
    seen: dict[str, str] = {}
    for record in records:
        module_id = record["id"]
        if module_id in seen:
            raise RegistryError(
                f"duplicate module id {module_id!r} from {record['download_url']} and {seen[module_id]}"
            )
        seen[module_id] = record["download_url"]


def generate_registry(modules_dir: Path, base_url: str) -> dict[str, Any]:
    if not modules_dir.exists():
        raise RegistryError(f"modules directory does not exist: {modules_dir}")
    if not modules_dir.is_dir():
        raise RegistryError(f"modules path is not a directory: {modules_dir}")

    module_paths = sorted(modules_dir.glob("*.rmod"), key=lambda path: path.name.lower())
    records = [build_module_record(path, parse_rmod(path), base_url) for path in module_paths]
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
    base_url = args.download_base_url or (
        f"https://raw.githubusercontent.com/{args.owner}/{args.repo}/{args.branch}/modules"
    )

    try:
        registry = generate_registry(args.modules_dir, base_url)
        write_registry(args.output, registry)
    except RegistryError as error:
        print(f"error: {error}")
        return 1

    print(f"wrote {args.output} with {len(registry['modules'])} module(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
