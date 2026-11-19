"""No-database mode: no Mongo or Redis configured, everything runs in memory."""
import json
from pathlib import Path

import httpx
import pytest

from app.cache import RedisCache
from app.config import Settings
from app.main import create_app
from app.memory import MemoryCache, MemoryRepository
from app.repo import MongoRepository

from .conftest import FILE_NAME, release_entry

BASE = "https://abc123.public.blob.vercel-storage.com/tributary"
UA = {"user-agent": "Mozilla/5.0 (Windows NT 10.0)"}
ENV = ("MONGO_URL", "MONGODB_URI", "REDIS_URL", "KV_URL", "DOWNLOAD_BASE_URL", "RELEASES_FILE",
       "SERVERLESS", "VERCEL")


@pytest.fixture
def clean_env(monkeypatch):
    for k in ENV:
        monkeypatch.delenv(k, raising=False)
    return monkeypatch


@pytest.fixture
async def nodb(clean_env, tmp_path: Path):
    manifest = tmp_path / "releases.json"
    manifest.write_text(json.dumps({"releases": [release_entry(), release_entry("0.2.0")]}))
    settings = Settings(
        downloads_dir=str(tmp_path / "none"), releases_file=str(manifest),
        download_base_url=BASE, count_salt="s",
    )
    app = create_app(settings)  # no repo, no cache: chosen from the settings
    async with app.router.lifespan_context(app):
        async with httpx.AsyncClient(
            transport=httpx.ASGITransport(app=app, client=("203.0.113.7", 1)),
            base_url="http://t", headers=UA,
        ) as c:
            yield c


async def test_health_ok_and_not_configured(nodb):
    r = await nodb.get("/api/health")
    assert r.status_code == 200
    body = r.json()
    assert body["status"] == "ok" and body["mongo"] == "not configured" and body["redis"] == "not configured"


async def test_releases_from_manifest(nodb):
    assert (await nodb.get("/api/releases/latest")).json()["version"] == "0.2.0"
    assert [r["version"] for r in (await nodb.get("/api/releases")).json()] == ["0.2.0", "0.1.0"]


async def test_download_redirect_and_stats(nodb):
    r = await nodb.get("/api/download/0.1.0")
    assert r.status_code == 302 and r.headers["location"] == f"{BASE}/{FILE_NAME}"
    assert (await nodb.get("/api/download/0.1.0")).status_code == 302  # same visitor, same day
    await nodb.get("/api/download/latest", headers={"user-agent": "curl/8"})
    assert (await nodb.get("/api/download/9.9.9")).status_code == 404
    d = (await nodb.get("/api/stats")).json()["downloads"]
    assert d == {"total": 2, "by_version": {"0.1.0": 1, "0.2.0": 1}}


def test_unconfigured_is_detected_but_compose_default_kept(clean_env):
    s = Settings()
    assert not s.mongo_configured and not s.redis_configured
    assert s.mongo_url == "mongodb://mongo:27017"


@pytest.mark.parametrize("var", ["MONGO_URL", "MONGODB_URI"])
def test_mongo_env_selects_real_client(clean_env, var):
    clean_env.setenv(var, "mongodb://localhost:1/x")
    s = Settings()
    assert s.mongo_configured
    repo = _built(s)[0]
    assert isinstance(repo, MongoRepository)


@pytest.mark.parametrize("var", ["REDIS_URL", "KV_URL"])
def test_redis_env_selects_real_client(clean_env, var):
    clean_env.setenv(var, "redis://localhost:1/0")
    s = Settings()
    assert s.redis_configured
    cache = _built(s)[1]
    assert isinstance(cache, RedisCache)


def test_blank_env_counts_as_unconfigured(clean_env):
    clean_env.setenv("MONGO_URL", " ")
    clean_env.setenv("REDIS_URL", "")
    s = Settings()
    assert not s.mongo_configured and not s.redis_configured


def test_unconfigured_selects_memory(clean_env):
    repo, cache = _built(Settings())
    assert isinstance(repo, MemoryRepository) and isinstance(cache, MemoryCache)


def _built(settings):
    """The repo and cache that create_app picks for these settings (without starting the app)."""
    captured = {}
    import app.main as m

    real_mem_r, real_mem_c, real_mongo, real_redis = m.MemoryRepository, m.MemoryCache, m.MongoRepository, m.RedisCache

    def spy(cls, key):
        def make(*a, **kw):
            obj = cls(*a, **kw)
            captured[key] = obj
            return obj
        return make

    m.MemoryRepository, m.MemoryCache = spy(real_mem_r, "repo"), spy(real_mem_c, "cache")
    m.MongoRepository, m.RedisCache = spy(real_mongo, "repo"), spy(real_redis, "cache")
    try:
        create_app(settings)
    finally:
        m.MemoryRepository, m.MemoryCache, m.MongoRepository, m.RedisCache = real_mem_r, real_mem_c, real_mongo, real_redis
    return captured["repo"], captured["cache"]


async def test_memory_cache_expires(monkeypatch):
    c = MemoryCache()
    await c.set("k", "v", 10)
    assert await c.get("k") == "v"
    c.expires["k"] = 0
    assert await c.get("k") is None
    assert await c.set_once("s", 5) and not await c.set_once("s", 5)
