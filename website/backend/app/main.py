import asyncio
import hashlib
import json
import logging
import socket
from contextlib import asynccontextmanager
from datetime import datetime, timezone
from pathlib import Path

from fastapi import FastAPI, HTTPException, Path as PathParam, Request, Response
from fastapi.responses import JSONResponse

from .cache import RedisCache
from .config import Settings
from .logging_setup import setup_logging
from .memory import MemoryCache, MemoryRepository
from .models import VERSION_PATTERN, Release
from .ports import Cache, Repository
from .repo import MongoRepository
from .seed import CACHE_KEYS, seed_releases

log = logging.getLogger("tributary.api")

KEY_LATEST, KEY_ALL = CACHE_KEYS
KEY_STATS = "stats"
PUBLIC_CACHE = "public, max-age=5"


def version_key(version: str) -> tuple[int, ...]:
    """A version such as 0.10.2 as numbers, so 0.10 sorts after 0.9."""
    return tuple(int(part) for part in version.split("."))


def ua_family(user_agent: str) -> str:
    return "windows" if "windows" in user_agent.lower() else "other"


APP_KWARGS = dict(title="Tributary website API", docs_url=None, redoc_url=None, openapi_url=None)


def create_app(
    settings: Settings | None = None,
    repo: Repository | None = None,
    cache: Cache | None = None,
) -> FastAPI:
    """Build a fresh app (used by the tests). The deployed app is the module-level `app` below."""
    return configure(FastAPI(**APP_KWARGS), settings, repo, cache)


def configure(
    app: FastAPI,
    settings: Settings | None = None,
    repo: Repository | None = None,
    cache: Cache | None = None,
) -> FastAPI:
    """Attach lifespan and routes to `app`. Nothing connects to Mongo or Redis here."""
    settings = settings or Settings()
    setup_logging(settings.log_level)
    lazy = settings.lazy_startup
    # No MONGO_URL / MONGODB_URI (or REDIS_URL / KV_URL) in the environment: run on in-memory
    # stand-ins. Releases come from the manifest; counts last only for this process.
    mongo_off = repo is None and not settings.mongo_configured
    redis_off = cache is None and not settings.redis_configured
    if repo is None:
        repo = MemoryRepository() if mongo_off else MongoRepository(
            settings.mongo_url, settings.mongo_db, settings.mongo_timeout_ms or (5000 if lazy else 2000)
        )
    if cache is None:
        cache = MemoryCache() if redis_off else RedisCache(settings.redis_url)
    downloads_dir = Path(settings.downloads_dir)
    manifest_path = settings.manifest_path
    redirect_base = settings.download_base_url
    instance = socket.gethostname()
    if lazy and settings.count_salt == "change-me":
        log.warning("COUNT_SALT is the default value; set a long random one")

    ready = False
    ready_lock: asyncio.Lock | None = None

    async def ensure_ready() -> None:
        """Serverless mode: index and seed once per instance, on first use. Failures retry on a later request."""
        nonlocal ready, ready_lock
        if not lazy or ready:
            return
        if ready_lock is None:
            ready_lock = asyncio.Lock()
        async with ready_lock:
            if ready:
                return
            try:
                await repo.ensure_indexes()
                await seed_releases(
                    repo, cache, downloads_dir, manifest_path, verify_files=redirect_base is None
                )
                ready = True
            except Exception as exc:
                log.warning("lazy initialisation failed; will retry", extra={"error": type(exc).__name__})

    async def initialise() -> None:
        """Connect, index and seed, retrying with backoff while Mongo or Redis come up."""
        delay = settings.startup_backoff_initial
        attempt = 0
        while True:
            attempt += 1
            try:
                await repo.ping()
                await cache.ping()
                await repo.ensure_indexes()
                await seed_releases(repo, cache, downloads_dir, manifest_path)
                log.info("startup complete", extra={"attempts": attempt})
                return
            except asyncio.CancelledError:
                raise
            except Exception as exc:
                log.warning(
                    "dependency not ready, retrying",
                    extra={"attempt": attempt, "retry_in": delay, "error": type(exc).__name__},
                )
                await asyncio.sleep(delay)
                delay = min(delay * 2, settings.startup_backoff_max)

    @asynccontextmanager
    async def lifespan(_: FastAPI):
        if lazy:
            try:
                yield
            finally:
                await asyncio.gather(repo.close(), cache.close(), return_exceptions=True)
            return
        task = asyncio.create_task(initialise())
        try:
            await asyncio.wait_for(asyncio.shield(task), timeout=settings.startup_wait_seconds)
        except asyncio.TimeoutError:
            log.error("starting without full dependencies; still retrying in the background")
        try:
            yield
        finally:
            task.cancel()
            await asyncio.gather(task, return_exceptions=True)
            await asyncio.gather(repo.close(), cache.close(), return_exceptions=True)

    app.router.lifespan_context = lifespan

    # ---- helpers -------------------------------------------------------------------------

    async def cache_get(key: str) -> str | None:
        try:
            return await cache.get(key)
        except Exception:
            log.warning("cache read failed", extra={"key": key})
            return None

    async def cache_set(key: str, value: str, ttl: int) -> None:
        try:
            await cache.set(key, value, ttl)
        except Exception:
            log.warning("cache write failed", extra={"key": key})

    def json_response(body: str, status: int = 200) -> Response:
        return Response(body, status_code=status, media_type="application/json",
                        headers={"Cache-Control": PUBLIC_CACHE})

    async def all_releases() -> list[Release]:
        cached = await cache_get(KEY_ALL)
        if cached is not None:
            return [Release.model_validate(r) for r in json.loads(cached)]
        releases = await repo.list_releases()
        await cache_set(KEY_ALL, json.dumps([r.model_dump() for r in releases]), settings.cache_ttl_releases)
        return releases

    async def latest_release() -> Release | None:
        cached = await cache_get(KEY_LATEST)
        if cached is not None:
            return Release.model_validate_json(cached)
        releases = await all_releases()
        if not releases:
            return None
        await cache_set(KEY_LATEST, releases[0].model_dump_json(), settings.cache_ttl_releases)
        return releases[0]

    async def ensure_counters() -> dict[str, int]:
        counters = await cache.counters()
        if counters is None:
            counts = await repo.download_counts()
            await cache.init_counters({"total": sum(counts.values()), **counts})
            counters = await cache.counters() or {"total": 0}
        return counters

    async def count_download(request: Request, version: str) -> None:
        """Count once per salted hash of IP + user agent + UTC day. No IP is ever stored."""
        ip = request.client.host if request.client else ""
        ua = request.headers.get("user-agent", "")
        now = datetime.now(timezone.utc)
        digest = hashlib.sha256(f"{settings.count_salt}|{ip}|{ua}|{now:%Y-%m-%d}".encode()).hexdigest()
        if not await cache.set_once(f"dl:seen:{digest}", settings.cache_ttl_seen):
            return
        await ensure_counters()  # before inserting, so a rebuild from Mongo cannot double count
        await repo.record_download(version, now, ua_family(ua))
        await cache.incr(version)
        await cache.delete(KEY_STATS)

    async def serve_installer(request: Request, release: Release | None) -> Response:
        if release is None:
            raise HTTPException(404, "Release not found")
        # The file name comes only from the manifest record (validated at load), never from the request.
        if redirect_base is not None:
            try:
                await count_download(request, release.version)
            except Exception:
                log.warning("could not count download", exc_info=True)
            return Response(
                status_code=302,
                headers={"Location": f"{redirect_base}/{release.file}", "Cache-Control": "no-store"},
            )
        if not (downloads_dir / release.file).is_file():
            log.error("installer file missing", extra={"version": release.version})
            raise HTTPException(404, "Release not found")
        try:
            await count_download(request, release.version)
        except Exception:
            log.warning("could not count download", exc_info=True)
        return Response(
            status_code=200,
            headers={
                "X-Accel-Redirect": f"/_files/{release.file}",
                "Content-Disposition": f'attachment; filename="{release.file}"',
                "Content-Type": "application/octet-stream",
                "Cache-Control": "no-store",
            },
        )

    # ---- routes --------------------------------------------------------------------------

    @app.get("/api/health")
    async def health() -> Response:
        async def probe(fn, off: bool) -> str:
            if off:
                return "not configured"
            try:
                await asyncio.wait_for(fn(), timeout=2)
                return "ok"
            except Exception:
                return "down"

        mongo, redis_ = await asyncio.gather(probe(repo.ping, mongo_off), probe(cache.ping, redis_off))
        ok = "down" not in (mongo, redis_)
        return JSONResponse(
            {"status": "ok" if ok else "degraded", "mongo": mongo, "redis": redis_, "instance": instance},
            status_code=200 if ok else 503,
            headers={"Cache-Control": "no-store"},
        )

    @app.get("/api/releases/latest")
    async def releases_latest() -> Response:
        await ensure_ready()
        release = await latest_release()
        if release is None:
            raise HTTPException(404, "No releases")
        return json_response(release.model_dump_json())

    @app.get("/api/releases")
    async def releases() -> Response:
        await ensure_ready()
        return json_response(json.dumps([r.model_dump() for r in await all_releases()]))

    @app.get("/api/download/latest")
    async def download_latest(request: Request) -> Response:
        await ensure_ready()
        return await serve_installer(request, await latest_release())

    @app.get("/api/download/{version}")
    async def download_version(
        request: Request, version: str = PathParam(pattern=VERSION_PATTERN)
    ) -> Response:
        await ensure_ready()
        release = next((r for r in await all_releases() if r.version == version), None)
        return await serve_installer(request, release)

    @app.get("/api/updates/{target}/{arch}/{current}")
    async def updates(
        request: Request, target: str, arch: str, current: str = PathParam(pattern=VERSION_PATTERN)
    ) -> Response:
        """What the app's updater asks. 204 means there's nothing newer for it."""
        await ensure_ready()
        release = await latest_release()
        up_to_date = Response(status_code=204, headers={"Cache-Control": "no-store"})
        if (
            release is None
            or not release.signature
            or (target, arch) != ("windows", "x86_64")
            or version_key(release.version) <= version_key(current)
        ):
            return up_to_date
        if redirect_base is not None:
            url = f"{redirect_base}/{release.file}"
        else:
            base = (settings.public_url or str(request.base_url)).rstrip("/")
            url = f"{base}/api/download/{release.version}"
        body = {
            "version": release.version,
            "notes": "\n".join(release.notes),
            "pub_date": f"{release.published}T00:00:00Z",
            "url": url,
            "signature": release.signature,
        }
        return Response(json.dumps(body), media_type="application/json",
                        headers={"Cache-Control": "no-store"})

    @app.get("/api/stats")
    async def stats() -> Response:
        await ensure_ready()
        cached = await cache_get(KEY_STATS)
        if cached is not None:
            return json_response(cached)
        counters = await ensure_counters()
        total = counters.get("total", 0)
        by_version = {k: v for k, v in sorted(counters.items()) if k != "total"}
        body = json.dumps({"downloads": {"total": total, "by_version": by_version}})
        await cache_set(KEY_STATS, body, settings.cache_ttl_stats)
        return json_response(body)

    return app


# The ASGI app that Vercel (top-level `app` in app/main.py) and the Docker image (`app.main:app`) run.
# Settings come from the environment; Mongo and Redis are only contacted when a request needs them.
app = FastAPI(**APP_KWARGS)
configure(app)
