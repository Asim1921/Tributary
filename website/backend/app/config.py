from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=None, extra="ignore")

    mongo_url: str = "mongodb://mongo:27017"
    mongo_db: str = "tributary"
    redis_url: str = "redis://redis:6379/0"
    downloads_dir: str = "/srv/downloads"
    count_salt: str = "change-me"
    cache_ttl_releases: int = Field(60, ge=0)
    cache_ttl_stats: int = Field(30, ge=0)
    cache_ttl_seen: int = Field(86400, ge=1)
    # Startup: how long to wait for the data services before serving anyway
    # (the app keeps retrying in the background; /api/health says 503 until ready).
    startup_wait_seconds: float = 60.0
    startup_backoff_initial: float = 0.5
    startup_backoff_max: float = 5.0
    log_level: str = "INFO"
