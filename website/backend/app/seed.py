"""Seed the release collection from downloads/releases.json."""
import asyncio
import json
import logging
from pathlib import Path

from pydantic import ValidationError

from .models import Manifest, Release
from .ports import Cache, Repository

log = logging.getLogger("tributary.seed")

CACHE_KEYS = ("rel:latest", "rel:all")


def load_manifest(downloads_dir: Path, path: Path | None = None) -> list[Release]:
    path = path or downloads_dir / "releases.json"
    if not path.is_file():
        log.warning("no release manifest", extra={"path": str(path)})
        return []
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
        return Manifest.model_validate(raw).releases
    except (OSError, ValueError, ValidationError):
        log.exception("release manifest is invalid", extra={"path": str(path)})
        return []


def file_matches(downloads_dir: Path, release: Release) -> bool:
    """The installer must exist next to the manifest with the recorded size."""
    root = downloads_dir.resolve()
    target = (root / release.file).resolve()
    if target.parent != root or not target.is_file():
        log.error("installer missing", extra={"version": release.version, "file": release.file})
        return False
    size = target.stat().st_size
    if size != release.size:
        log.error(
            "installer size differs from manifest",
            extra={"version": release.version, "file": release.file, "expected": release.size, "actual": size},
        )
        return False
    return True


async def seed_releases(
    repo: Repository,
    cache: Cache,
    downloads_dir: Path,
    manifest_path: Path | None = None,
    verify_files: bool = True,
) -> int:
    """Upsert every valid release (idempotent, safe with several replicas). Returns the count seeded.

    With verify_files=False (installer hosted elsewhere) the manifest alone is trusted: size and
    SHA-256 come from it and the installer need not exist on disk.
    """
    releases = await asyncio.to_thread(load_manifest, downloads_dir, manifest_path)
    seeded = 0
    for release in releases:
        if not verify_files or await asyncio.to_thread(file_matches, downloads_dir, release):
            await repo.upsert_release(release)
            seeded += 1
    if seeded:
        try:
            await cache.delete(*CACHE_KEYS)
        except Exception:
            log.warning("could not invalidate release cache", exc_info=True)
    log.info("releases seeded", extra={"seeded": seeded, "in_manifest": len(releases)})
    return seeded
