import asyncio
import json

import httpx
import pytest

from app.config import Settings
from app.main import create_app
from app.models import Release
from app.seed import load_manifest, seed_releases

from .conftest import FILE_NAME, INSTALLER, release_entry
from .fakes import FakeCache, FakeRepo

UA_WIN = {"user-agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"}


# ---- health ------------------------------------------------------------------------------

async def test_health_ok(client):
    r = await client.get("/api/health")
    assert r.status_code == 200
    body = r.json()
    assert body["status"] == "ok" and body["mongo"] == "ok" and body["redis"] == "ok"
    assert body["instance"]
    assert r.headers["cache-control"] == "no-store"


async def test_health_503_when_mongo_down(client, repo):
    repo.down = True
    r = await client.get("/api/health")
    assert r.status_code == 503
    assert r.json()["mongo"] == "down" and r.json()["redis"] == "ok"
    assert r.json()["status"] == "degraded"


async def test_health_503_when_redis_down(client, cache):
    cache.down = True
    r = await client.get("/api/health")
    assert r.status_code == 503
    assert r.json()["redis"] == "down" and r.json()["mongo"] == "ok"


# ---- releases ----------------------------------------------------------------------------

async def test_latest_release(client):
    r = await client.get("/api/releases/latest")
    assert r.status_code == 200
    body = r.json()
    assert set(body) == {"version", "file", "size", "sha256", "published", "notes", "requirements", "signature"}
    assert body["version"] == "0.1.0" and body["file"] == FILE_NAME and body["size"] == len(INSTALLER)
    assert r.headers["content-type"] == "application/json"


async def test_latest_404_when_no_releases(settings, repo, cache, tmp_path):
    empty = tmp_path / "empty"
    empty.mkdir()
    settings.downloads_dir = str(empty)  # no manifest at all
    app = create_app(settings, repo, cache)
    async with app.router.lifespan_context(app):
        async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://t") as c:
            assert (await c.get("/api/releases/latest")).status_code == 404
            assert (await c.get("/api/releases")).json() == []
            assert (await c.get("/api/download/latest")).status_code == 404


async def test_releases_sorted_newest_first_numerically(client, repo, downloads, cache):
    for v in ("0.9.0", "0.10.0", "1.0.0"):
        (downloads / f"Tributary_{v}_x64-setup.exe").write_bytes(INSTALLER)
        await repo.upsert_release(Release(**release_entry(v, f"Tributary_{v}_x64-setup.exe")))
    await cache.delete("rel:all", "rel:latest")
    r = await client.get("/api/releases")
    assert [x["version"] for x in r.json()] == ["1.0.0", "0.10.0", "0.9.0", "0.1.0"]
    assert (await client.get("/api/releases/latest")).json()["version"] == "1.0.0"


async def test_releases_cached_in_redis_for_ttl(client, repo, cache):
    await client.get("/api/releases")
    await client.get("/api/releases")
    await client.get("/api/releases/latest")
    await client.get("/api/releases/latest")
    assert repo.list_calls == 1
    assert cache.ttls["rel:all"] == 60 and cache.ttls["rel:latest"] == 60


async def test_cache_ttl_comes_from_settings(settings, repo, cache):
    settings.cache_ttl_releases = 7
    app = create_app(settings, repo, cache)
    async with app.router.lifespan_context(app):
        async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://t") as c:
            await c.get("/api/releases/latest")
    assert cache.ttls["rel:latest"] == 7


async def test_reseed_invalidates_release_cache(client, repo, cache, settings, downloads):
    first = (await client.get("/api/releases/latest")).json()
    assert "rel:latest" in cache.kv
    (downloads / "releases.json").write_text(
        json.dumps({"releases": [release_entry(notes=["Changed notes."])]})
    )
    await seed_releases(repo, cache, downloads)
    assert "rel:latest" not in cache.kv and "rel:all" not in cache.kv
    second = (await client.get("/api/releases/latest")).json()
    assert first["notes"] == ["First release."] and second["notes"] == ["Changed notes."]


async def test_releases_still_served_when_redis_down(client, cache):
    cache.down = True
    r = await client.get("/api/releases/latest")
    assert r.status_code == 200 and r.json()["version"] == "0.1.0"


# ---- download ----------------------------------------------------------------------------

async def test_download_latest_hands_off_to_nginx(client):
    r = await client.get("/api/download/latest")
    assert r.status_code == 200
    assert r.headers["x-accel-redirect"] == f"/_files/{FILE_NAME}"
    assert r.headers["content-disposition"] == f'attachment; filename="{FILE_NAME}"'
    assert r.headers["cache-control"] == "no-store"
    assert r.content == b""


async def test_download_specific_version(client):
    r = await client.get("/api/download/0.1.0")
    assert r.status_code == 200 and r.headers["x-accel-redirect"] == f"/_files/{FILE_NAME}"


@pytest.mark.parametrize("version", ["9.9.9", "0.1.1", "0.0.0"])
async def test_download_unknown_version_404(client, version):
    r = await client.get(f"/api/download/{version}")
    assert r.status_code == 404
    assert "x-accel-redirect" not in r.headers


@pytest.mark.parametrize(
    "version",
    ["1.0", "1.0.0.0", "v1.0.0", "abc", "1.0.0-beta", "..", "%2e%2e", "1.0.0%00", "a.b.c", " 1.0.0", "1.0.0/../x"],
)
async def test_download_malformed_version_rejected_and_never_hands_off(client, version):
    r = await client.get(f"/api/download/{version}")
    assert r.status_code in (404, 422)
    assert "x-accel-redirect" not in r.headers


async def test_download_file_name_never_from_request(client):
    for path in ("/api/download/..%2f..%2fetc%2fpasswd", "/api/download/latest?file=../../secret",
                 f"/api/download/0.1.0?file=evil.exe"):
        r = await client.get(path)
        assert "evil" not in r.headers.get("x-accel-redirect", "")
        assert "secret" not in r.headers.get("x-accel-redirect", "")
        if r.status_code == 200:
            assert r.headers["x-accel-redirect"] == f"/_files/{FILE_NAME}"


async def test_download_404_when_installer_missing_on_disk(client, downloads):
    (downloads / FILE_NAME).unlink()
    r = await client.get("/api/download/latest")
    assert r.status_code == 404


async def test_download_works_when_counting_fails(client, cache):
    cache.down = True  # releases come from Mongo, counting is skipped, the file still goes out
    r = await client.get("/api/download/latest")
    assert r.status_code == 200 and "x-accel-redirect" in r.headers


# ---- counting and privacy ----------------------------------------------------------------

async def test_download_counted_once_per_ip_ua_day(client, repo, cache):
    for _ in range(3):
        assert (await client.get("/api/download/latest")).status_code == 200
    assert len(repo.downloads) == 1
    assert cache.hash == {"total": 1, "0.1.0": 1}
    # a second path to the same release by the same visitor on the same day does not count either
    await client.get("/api/download/0.1.0")
    assert len(repo.downloads) == 1


async def test_different_visitors_count_separately(client, repo):
    await client.get("/api/download/latest")
    await client.get("/api/download/latest", headers={"user-agent": "curl/8.0"})
    assert len(repo.downloads) == 2
    assert {d["ua_family"] for d in repo.downloads} == {"windows", "other"}


async def test_different_ip_counts_separately(settings, repo, cache):
    app = create_app(settings, repo, cache)
    async with app.router.lifespan_context(app):
        for ip in ("198.51.100.1", "198.51.100.2"):
            transport = httpx.ASGITransport(app=app, client=(ip, 1))
            async with httpx.AsyncClient(transport=transport, base_url="http://t", headers=UA_WIN) as c:
                await c.get("/api/download/latest")
    assert len(repo.downloads) == 2


async def test_day_rollover_counts_again(client, repo, cache):
    await client.get("/api/download/latest")
    (key,) = [k for k in cache.kv if k.startswith("dl:seen:")]
    assert cache.ttls[key] == 86400  # the dedupe key lives one day, so tomorrow it counts again
    del cache.kv[key]
    await client.get("/api/download/latest")
    assert len(repo.downloads) == 2


async def test_no_ip_stored_anywhere(client, repo, cache):
    await client.get("/api/download/latest")
    haystack = json.dumps([{**d, "at": str(d["at"])} for d in repo.downloads]) + json.dumps(cache.kv) + json.dumps(cache.hash)
    assert "203.0.113.7" not in haystack
    assert "Mozilla" not in haystack
    assert set(repo.downloads[0]) == {"version", "at", "ua_family"}
    (key,) = [k for k in cache.kv if k.startswith("dl:seen:")]
    assert len(key.removeprefix("dl:seen:")) == 64  # salted SHA-256 hex only


async def test_salt_changes_the_hash(settings, downloads):
    keys = []
    for salt in ("a", "b"):
        cache = FakeCache()
        s = settings.model_copy(update={"count_salt": salt})
        app = create_app(s, FakeRepo(), cache)
        async with app.router.lifespan_context(app):
            async with httpx.AsyncClient(
                transport=httpx.ASGITransport(app=app, client=("1.2.3.4", 1)), base_url="http://t", headers=UA_WIN
            ) as c:
                await c.get("/api/download/latest")
        keys.append([k for k in cache.kv if k.startswith("dl:seen:")][0])
    assert keys[0] != keys[1]


# ---- stats -------------------------------------------------------------------------------

async def test_stats_empty(client):
    r = await client.get("/api/stats")
    assert r.status_code == 200
    assert r.json() == {"downloads": {"total": 0, "by_version": {}}}


async def test_stats_counts_and_cache_invalidated_by_new_download(client, cache):
    await client.get("/api/stats")
    assert cache.ttls["stats"] == 30
    await client.get("/api/download/latest")
    assert "stats" not in cache.kv
    assert (await client.get("/api/stats")).json() == {"downloads": {"total": 1, "by_version": {"0.1.0": 1}}}


async def test_stats_cached_for_ttl(client, repo, cache):
    await client.get("/api/stats")
    cache.hash.update({"total": 99})  # changes behind the cache; the cached body must win
    assert (await client.get("/api/stats")).json()["downloads"]["total"] == 0


async def test_stats_rebuilt_from_mongo_when_redis_empty(client, repo, cache):
    for ua in ("a", "b", "c"):
        await client.get("/api/download/latest", headers={"user-agent": ua})
    cache.flush()  # Redis restarted empty
    r = await client.get("/api/stats")
    assert r.json() == {"downloads": {"total": 3, "by_version": {"0.1.0": 3}}}
    assert cache.hash == {"total": 3, "0.1.0": 3}


async def test_counting_after_redis_flush_does_not_lose_or_double_count(client, repo, cache):
    await client.get("/api/download/latest", headers={"user-agent": "a"})
    cache.flush()
    await client.get("/api/download/latest", headers={"user-agent": "b"})
    assert cache.hash == {"total": 2, "0.1.0": 2}
    assert len(repo.downloads) == 2


# ---- seeding and startup -----------------------------------------------------------------

async def test_startup_seeds_and_creates_indexes(client, repo):
    assert list(repo.releases) == ["0.1.0"]
    assert repo.indexes == ["releases.version unique", "downloads.at"]


async def test_seed_is_idempotent(repo, cache, downloads):
    for _ in range(3):
        await seed_releases(repo, cache, downloads)
    assert len(repo.releases) == 1


async def test_seed_concurrent_replicas(repo, cache, downloads):
    await asyncio.gather(*(seed_releases(repo, cache, downloads) for _ in range(4)))
    assert len(repo.releases) == 1


async def test_seed_skips_release_with_missing_file(repo, cache, downloads):
    (downloads / FILE_NAME).unlink()
    assert await seed_releases(repo, cache, downloads) == 0
    assert repo.releases == {}


async def test_seed_skips_release_with_wrong_size(repo, cache, downloads):
    (downloads / FILE_NAME).write_bytes(INSTALLER + b"x")
    assert await seed_releases(repo, cache, downloads) == 0


@pytest.mark.parametrize("bad", ["../evil.exe", "..\\evil.exe", "sub/evil.exe", "/etc/passwd", ".hidden", "a b.exe", ""])
def test_manifest_rejects_unsafe_file_names(downloads, bad):
    (downloads / "releases.json").write_text(json.dumps({"releases": [release_entry(file=bad)]}))
    assert load_manifest(downloads) == []


def test_manifest_rejects_bad_version_and_hash(downloads):
    (downloads / "releases.json").write_text(json.dumps({"releases": [release_entry("1.0")]}))
    assert load_manifest(downloads) == []
    (downloads / "releases.json").write_text(json.dumps({"releases": [release_entry(sha256="zz")]}))
    assert load_manifest(downloads) == []


def test_manifest_missing_or_corrupt_is_empty(tmp_path):
    assert load_manifest(tmp_path) == []
    (tmp_path / "releases.json").write_text("{not json")
    assert load_manifest(tmp_path) == []


async def test_startup_retries_until_dependencies_are_up(settings, repo, cache):
    repo.down = True
    cache.down = True
    settings.startup_wait_seconds = 0.05  # app starts serving while still waiting
    app = create_app(settings, repo, cache)
    async with app.router.lifespan_context(app):
        async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://t") as c:
            assert (await c.get("/api/health")).status_code == 503
            repo.down = False
            cache.down = False
            for _ in range(100):
                if repo.releases:
                    break
                await asyncio.sleep(0.02)
            assert list(repo.releases) == ["0.1.0"]
            assert (await c.get("/api/health")).status_code == 200


async def test_startup_waits_for_late_dependency_before_serving(settings, repo, cache):
    repo.down = True

    async def recover():
        await asyncio.sleep(0.1)
        repo.down = False

    task = asyncio.create_task(recover())
    app = create_app(settings, repo, cache)
    async with app.router.lifespan_context(app):
        assert list(repo.releases) == ["0.1.0"]
    await task


# ---- misc --------------------------------------------------------------------------------

async def test_docs_not_exposed_and_unknown_api_path_404(client):
    assert (await client.get("/docs")).status_code == 404
    assert (await client.get("/api/nope")).status_code == 404


async def test_settings_from_env(monkeypatch):
    monkeypatch.setenv("MONGO_URL", "mongodb://m:1")
    monkeypatch.setenv("REDIS_URL", "redis://r:2/0")
    monkeypatch.setenv("DOWNLOADS_DIR", "/x")
    monkeypatch.setenv("COUNT_SALT", "s3")
    monkeypatch.setenv("CACHE_TTL_RELEASES", "11")
    monkeypatch.setenv("CACHE_TTL_STATS", "22")
    s = Settings()
    assert (s.mongo_url, s.redis_url, s.downloads_dir, s.count_salt) == ("mongodb://m:1", "redis://r:2/0", "/x", "s3")
    assert (s.cache_ttl_releases, s.cache_ttl_stats) == (11, 22)
