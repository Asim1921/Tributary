"""Serverless / Vercel mode: module-level app, 302 downloads, lazy seeding, env fallbacks."""
import json
from pathlib import Path

import httpx
import pytest
from fastapi import FastAPI

from app.cache import RedisCache
from app.config import Settings
from app.main import create_app
from app.seed import load_manifest, seed_releases

from .conftest import FILE_NAME, release_entry
from .fakes import FakeCache, FakeRepo

BASE = "https://abc123.public.blob.vercel-storage.com/tributary"
UA = {"user-agent": "Mozilla/5.0 (Windows NT 10.0)"}


@pytest.fixture
def manifest_only(tmp_path: Path) -> Path:
    """A manifest and NO installer on disk, as on the API project."""
    path = tmp_path / "manifest" / "releases.json"
    path.parent.mkdir()
    path.write_text(json.dumps({"releases": [release_entry()]}))
    return path


@pytest.fixture
def r_settings(tmp_path, manifest_only) -> Settings:
    return Settings(
        downloads_dir=str(tmp_path / "nope"),
        releases_file=str(manifest_only),
        download_base_url=BASE + "/",  # trailing slash is normalised
        count_salt="test-salt",
    )


@pytest.fixture
async def rclient(r_settings, repo, cache):
    app = create_app(r_settings, repo, cache)
    async with app.router.lifespan_context(app):
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=app, client=("203.0.113.7", 5000)),
            base_url="http://test",
            headers=UA,
        ) as c:
            yield c


# ---- module-level app --------------------------------------------------------------------

async def test_module_level_app_serves_health():
    from app.main import app

    assert isinstance(app, FastAPI)
    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://t") as c:
        r = await c.get("/api/health")
    assert r.status_code in (200, 503)  # 503 when no Mongo/Redis is reachable, as in a bare test run
    assert set(r.json()) == {"status", "mongo", "redis", "instance"}


# ---- redirect mode -----------------------------------------------------------------------

async def test_redirect_latest(rclient, repo):
    r = await rclient.get("/api/download/latest")
    assert r.status_code == 302
    assert r.headers["location"] == f"{BASE}/{FILE_NAME}"
    assert r.headers["cache-control"] == "no-store"
    assert "x-accel-redirect" not in r.headers
    assert len(repo.downloads) == 1


async def test_redirect_version_counts_once_per_day(rclient, repo, cache):
    for path in ("/api/download/0.1.0", "/api/download/latest", "/api/download/0.1.0"):
        assert (await rclient.get(path)).status_code == 302
    assert len(repo.downloads) == 1
    assert cache.hash == {"total": 1, "0.1.0": 1}
    assert (await rclient.get("/api/stats")).json()["downloads"]["total"] == 1


async def test_redirect_counts_other_visitor(rclient, repo):
    await rclient.get("/api/download/latest")
    await rclient.get("/api/download/latest", headers={"user-agent": "curl/8"})
    assert len(repo.downloads) == 2


async def test_redirect_unknown_version_404(rclient, repo):
    r = await rclient.get("/api/download/9.9.9")
    assert r.status_code == 404 and "location" not in r.headers
    assert repo.downloads == []


@pytest.mark.parametrize("version", ["1.0", "v1.0.0", "abc", "1.0.0-beta", "..", "%2e%2e", "1.0.0%00", "1.0.0/../x"])
async def test_redirect_malformed_version_rejected(rclient, version):
    r = await rclient.get(f"/api/download/{version}")
    assert r.status_code in (404, 422) and "location" not in r.headers


async def test_redirect_target_never_from_request(rclient):
    r = await rclient.get("/api/download/latest?file=evil.exe&base=https://evil.example")
    assert r.headers["location"] == f"{BASE}/{FILE_NAME}"


async def test_redirect_still_redirects_when_counting_fails(rclient, cache):
    cache.down = True
    r = await rclient.get("/api/download/latest")
    assert r.status_code == 302 and r.headers["location"].endswith(FILE_NAME)


async def test_x_accel_mode_unchanged_without_base_url(client):
    r = await client.get("/api/download/latest")
    assert r.status_code == 200
    assert r.headers["x-accel-redirect"] == f"/_files/{FILE_NAME}"
    assert "location" not in r.headers


# ---- lazy startup and seeding without the installer ---------------------------------------

async def test_lifespan_does_not_touch_dependencies(r_settings):
    repo, cache = FakeRepo(), FakeCache()
    repo.down = cache.down = True  # would block or fail an eager startup
    app = create_app(r_settings, repo, cache)
    async with app.router.lifespan_context(app):
        assert repo.releases == {}


async def test_seeds_once_per_instance_on_first_use_without_installer(rclient, repo):
    assert repo.releases == {}  # nothing at startup
    assert (await rclient.get("/api/releases/latest")).json()["version"] == "0.1.0"
    assert list(repo.releases) == ["0.1.0"] and repo.indexes
    repo.indexes = []
    await rclient.get("/api/releases")
    await rclient.get("/api/download/latest")
    assert repo.indexes == []  # not repeated


async def test_lazy_init_retries_after_failure(r_settings, cache):
    repo = FakeRepo()
    repo.down = True
    app = create_app(r_settings, repo, cache)
    async with app.router.lifespan_context(app):
        async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://t") as c:
            with pytest.raises(Exception):
                await c.get("/api/releases/latest")
            repo.down = False
            assert (await c.get("/api/releases/latest")).status_code == 200
    assert list(repo.releases) == ["0.1.0"]


async def test_seed_without_installer_when_not_verifying(manifest_only, tmp_path, repo, cache):
    assert await seed_releases(repo, cache, tmp_path / "none", manifest_only, verify_files=False) == 1
    repo2 = FakeRepo()
    assert await seed_releases(repo2, cache, tmp_path / "none", manifest_only) == 0  # default still verifies


async def test_seed_twice_is_idempotent(manifest_only, tmp_path, repo, cache):
    for _ in range(2):
        await seed_releases(repo, cache, tmp_path, manifest_only, verify_files=False)
    assert len(repo.releases) == 1


# ---- settings and env fallbacks ----------------------------------------------------------

@pytest.fixture
def clean_env(monkeypatch):
    for k in ("MONGO_URL", "MONGODB_URI", "REDIS_URL", "KV_URL", "DOWNLOAD_BASE_URL", "RELEASES_FILE",
              "SERVERLESS", "VERCEL", "MONGO_TIMEOUT_MS"):
        monkeypatch.delenv(k, raising=False)
    return monkeypatch


def test_mongo_env_fallback(clean_env):
    clean_env.setenv("MONGODB_URI", "mongodb+srv://u:p@cluster.mongodb.net/?retryWrites=true")
    assert Settings().mongo_url.startswith("mongodb+srv://u:p@cluster")
    clean_env.setenv("MONGO_URL", "mongodb://m:1")
    assert Settings().mongo_url == "mongodb://m:1"  # MONGO_URL wins


def test_redis_env_fallback(clean_env):
    clean_env.setenv("KV_URL", "rediss://default:tok@host.upstash.io:6379")
    assert Settings().redis_url == "rediss://default:tok@host.upstash.io:6379"
    clean_env.setenv("REDIS_URL", "redis://r:2/0")
    assert Settings().redis_url == "redis://r:2/0"  # REDIS_URL wins


def test_defaults_unchanged_for_compose(clean_env):
    s = Settings()
    assert (s.mongo_url, s.redis_url, s.download_base_url) == ("mongodb://mongo:27017", "redis://redis:6379/0", None)
    assert s.lazy_startup is False
    assert s.manifest_path == Path(s.downloads_dir) / "releases.json"


def test_lazy_mode_selection(clean_env):
    clean_env.setenv("VERCEL", "1")
    assert Settings().lazy_startup is True
    clean_env.delenv("VERCEL")
    clean_env.setenv("DOWNLOAD_BASE_URL", BASE)
    s = Settings()
    assert s.lazy_startup is True and s.download_base_url == BASE
    assert s.manifest_path.name == "releases.json" and s.manifest_path.parent.name == "backend"
    assert s.manifest_path.is_file()  # the copy that ships with the API project
    clean_env.setenv("SERVERLESS", "false")
    assert Settings().lazy_startup is False
    clean_env.setenv("RELEASES_FILE", "/x/r.json")
    assert Settings().manifest_path == Path("/x/r.json")


def test_blank_and_bad_base_url(clean_env):
    clean_env.setenv("DOWNLOAD_BASE_URL", "  ")
    assert Settings().download_base_url is None
    clean_env.setenv("DOWNLOAD_BASE_URL", "ftp://x/y")
    with pytest.raises(ValueError):
        Settings()


def test_rediss_url_uses_tls():
    cache = RedisCache("rediss://default:tok@host.upstash.io:6379")
    pool = cache._r.connection_pool
    assert "ssl" in pool.connection_class.__name__.lower()
    assert pool.connection_kwargs["host"] == "host.upstash.io"


def test_bundled_manifest_is_valid():
    path = Path(__file__).resolve().parent.parent / "releases.json"
    assert [r.version for r in load_manifest(path.parent, path)]
