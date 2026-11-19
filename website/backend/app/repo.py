from datetime import datetime

from pymongo import ASCENDING, AsyncMongoClient
from pymongo.errors import DuplicateKeyError

from .models import Release, version_key


class MongoRepository:
    def __init__(self, url: str, db_name: str, timeout_ms: int = 2000) -> None:
        self._client: AsyncMongoClient = AsyncMongoClient(
            url, serverSelectionTimeoutMS=timeout_ms, connectTimeoutMS=timeout_ms, tz_aware=True
        )
        self._db = self._client[db_name]

    async def ping(self) -> None:
        await self._client.admin.command("ping")

    async def ensure_indexes(self) -> None:
        await self._db.releases.create_index([("version", ASCENDING)], unique=True, name="version_unique")
        await self._db.downloads.create_index([("at", ASCENDING)], name="at")

    async def upsert_release(self, release: Release) -> None:
        doc = release.model_dump()
        for attempt in range(2):
            try:
                await self._db.releases.update_one({"version": release.version}, {"$set": doc}, upsert=True)
                return
            except DuplicateKeyError:
                # Another replica inserted the same version at the same moment; retry as an update.
                if attempt:
                    raise

    async def list_releases(self) -> list[Release]:
        docs = await self._db.releases.find({}, {"_id": 0}).to_list(None)
        releases = [Release.model_validate(d) for d in docs]
        releases.sort(key=lambda r: version_key(r.version), reverse=True)
        return releases

    async def get_release(self, version: str) -> Release | None:
        doc = await self._db.releases.find_one({"version": version}, {"_id": 0})
        return Release.model_validate(doc) if doc else None

    async def record_download(self, version: str, at: datetime, ua_family: str) -> None:
        await self._db.downloads.insert_one({"version": version, "at": at, "ua_family": ua_family})

    async def download_counts(self) -> dict[str, int]:
        cursor = await self._db.downloads.aggregate([{"$group": {"_id": "$version", "n": {"$sum": 1}}}])
        return {d["_id"]: d["n"] async for d in cursor}

    async def close(self) -> None:
        await self._client.close()
