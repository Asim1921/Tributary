import redis.asyncio as redis

COUNTER_HASH = "dl:counts"


class RedisCache:
    def __init__(self, url: str) -> None:
        self._r = redis.from_url(url, decode_responses=True, socket_connect_timeout=2, socket_timeout=2)

    async def ping(self) -> None:
        await self._r.ping()

    async def get(self, key: str) -> str | None:
        return await self._r.get(key)

    async def set(self, key: str, value: str, ttl: int) -> None:
        if ttl > 0:
            await self._r.set(key, value, ex=ttl)

    async def delete(self, *keys: str) -> None:
        if keys:
            await self._r.delete(*keys)

    async def set_once(self, key: str, ttl: int) -> bool:
        return bool(await self._r.set(key, "1", nx=True, ex=ttl))

    async def counters(self) -> dict[str, int] | None:
        data = await self._r.hgetall(COUNTER_HASH)
        if not data or "total" not in data:
            return None
        return {k: int(v) for k, v in data.items()}

    async def init_counters(self, values: dict[str, int]) -> None:
        pipe = self._r.pipeline()
        for field, n in values.items():
            pipe.hsetnx(COUNTER_HASH, field, n)
        await pipe.execute()

    async def incr(self, version: str) -> None:
        pipe = self._r.pipeline()
        pipe.hincrby(COUNTER_HASH, version, 1)
        pipe.hincrby(COUNTER_HASH, "total", 1)
        await pipe.execute()

    async def close(self) -> None:
        await self._r.aclose()
