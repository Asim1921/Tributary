import hashlib
import json
from pathlib import Path

import httpx
import pytest

from app.config import Settings
from app.main import create_app

from .fakes import FakeCache, FakeRepo

INSTALLER = b"MZ" + b"tributary-test-installer" * 50
FILE_NAME = "Tributary_0.1.0_x64-setup.exe"


def release_entry(version="0.1.0", file=FILE_NAME, data=INSTALLER, **extra):
    return {
        "version": version,
        "file": file,
        "size": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "published": "2026-11-19",
        "notes": ["First release."],
        "requirements": ["Windows 10 or 11, 64-bit"],
        **extra,
    }


@pytest.fixture
def downloads(tmp_path: Path) -> Path:
    (tmp_path / FILE_NAME).write_bytes(INSTALLER)
    (tmp_path / "releases.json").write_text(json.dumps({"releases": [release_entry()]}))
    return tmp_path


@pytest.fixture
def repo() -> FakeRepo:
    return FakeRepo()


@pytest.fixture
def cache() -> FakeCache:
    return FakeCache()


@pytest.fixture
def settings(downloads: Path) -> Settings:
    return Settings(
        downloads_dir=str(downloads),
        count_salt="test-salt",
        startup_wait_seconds=5,
        startup_backoff_initial=0.01,
        startup_backoff_max=0.02,
    )


@pytest.fixture
async def client(settings, repo, cache):
    app = create_app(settings, repo, cache)
    async with app.router.lifespan_context(app):
        transport = httpx.ASGITransport(app=app, client=("203.0.113.7", 5000))
        async with httpx.AsyncClient(
            transport=transport, base_url="http://test", headers={"user-agent": "Mozilla/5.0 (Windows NT 10.0)"}
        ) as c:
            yield c
