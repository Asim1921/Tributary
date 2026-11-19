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
from .models import VERSION_PATTERN, Release
from .ports import Cache, Repository
from .repo import MongoRepository
from .seed import CACHE_KEYS, seed_releases

log = logging.getLogger("tributary.api")

KEY_LATEST, KEY_ALL = CACHE_KEYS
KEY_STATS = "stats"
PUBLIC_CACHE = "public, max-age=5"


def ua_family(user_agent: str) -> str:
    return "windows" if "windows" in user_agent.lower() else "other"


def create_app(
    settings: Settings | None = None,
    repo: Repository | None = None,
    cache: Cache | None = None,
) -> FastAPI:
    settings = settings or Settings()
    setup_logging(settings.log_level)
    repo = repo or MongoRepository(settings.mongo_url, settings.mongo_db)
    cache = cache or RedisCache(settings.redis_url)
    downloads_dir = Path(settings.downloads_dir)
    instance = socket.gethostname()

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
                await seed_releases(repo, cache, downloads_dir)
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

    app = FastAPI(
        title="Tributary website API",
        lifespan=lifespan,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
    )

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
        async def probe(fn) -> str:
            try:
                await asyncio.wait_for(fn(), timeout=2)
                return "ok"
            except Exception:
                return "down"

        mongo, redis_ = await asyncio.gather(probe(repo.ping), probe(cache.ping))
        ok = mongo == "ok" and redis_ == "ok"
        return JSONResponse(
            {"status": "ok" if ok else "degraded", "mongo": mongo, "redis": redis_, "instance": instance},
            status_code=200 if ok else 503,
            headers={"Cache-Control": "no-store"},
        )

    @app.get("/api/releases/latest")
    async def releases_latest() -> Response:
        release = await latest_release()
        if release is None:
            raise HTTPException(404, "No releases")
        return json_response(release.model_dump_json())

    @app.get("/api/releases")
    async def releases() -> Response:
        return json_response(json.dumps([r.model_dump() for r in await all_releases()]))

    @app.get("/api/download/latest")
    async def download_latest(request: Request) -> Response:
        return await serve_installer(request, await latest_release())

    @app.get("/api/download/{version}")
    async def download_version(
        request: Request, version: str = PathParam(pattern=VERSION_PATTERN)
    ) -> Response:
        release = next((r for r in await all_releases() if r.version == version), None)
        return await serve_installer(request, release)

    @app.get("/api/stats")
    async def stats() -> Response:
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
