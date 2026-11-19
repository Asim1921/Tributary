# Tributary website

The public website for Tributary, the download manager for Windows in this repository. It is a
small production-style stack: a Next.js front end, a FastAPI back end, MongoDB, Redis, and nginx
in front doing load balancing, caching, rate limiting and file serving. The full design and
contract is in [SPEC.md](SPEC.md).

```
                      browser
                         |
                  :8080  |  (the only published port)
                  +------v------+
                  |    nginx    |  cache, gzip, rate limits, security headers,
                  |  (edge)     |  JSON access log, serves installers (X-Accel-Redirect)
                  +--+-------+--+
        least_conn   |       |   least_conn
            +--------v--+  +-v---------+
            | web  x2   |  | api  x2   |   network "edge"
            | Next.js   |  | FastAPI   |
            +-----------+  +--+-----+--+
                              |     |        network "data" (internal, no outside access)
                        +-----v-+ +-v-----+
                        | mongo | | redis |
                        +-------+ +-------+
```

| Service | Image | Replicas | Port (inside) |
|---|---|---|---|
| nginx | `nginx:1.28-alpine` | 1 | 80, published as `${SITE_PORT:-8080}` |
| web | built from `frontend/` (Node 22 alpine, Next.js standalone) | 2 | 3000 |
| api | built from `backend/` (Python 3.13 slim, uvicorn) | 2 | 8000 |
| mongo | `mongo:8.0` | 1 | 27017, not published |
| redis | `redis:7.4-alpine` | 1 | 6379, not published |

## Prerequisites

- Docker Desktop (Linux containers) with Compose v2.
- Node 22 for the smoke test and the front end's tests; Python 3.13 for the back end's tests.
- PowerShell, to stage a release. About 2 GB of free disk for images.

## Quick start

```powershell
cd website
Copy-Item .env.example .env            # then set COUNT_SALT to a long random value
.\scripts\publish-installer.ps1        # stage the installer and releases.json (see below)
docker compose up -d --build
# open http://localhost:8080
docker compose ps                      # every service should say "healthy"
docker compose down                    # stop (data volumes are kept); add -v to delete them
```

`.env` is git-ignored. `COUNT_SALT` is mandatory: compose refuses to start without it.

### Staging a release

The installers are not in git. Build the app (`npm run tauri build` in the repository root),
then run `scripts/publish-installer.ps1 [-Version 0.1.0]`. It copies
`../target/release/bundle/nsis/Tributary_<v>_x64-setup.exe` into `downloads/`, computes the size
and SHA-256, and upserts the entry in `downloads/releases.json`. The API reads that file at
startup (idempotent, safe with two replicas), so restart it after staging:
`docker compose restart api`.

`downloads/` is mounted read-only into nginx and the API at `/srv/downloads`.

## Ports

| Port | What | Reachable from |
|---|---|---|
| 8080 (`SITE_PORT`) | nginx | your machine |
| 80, 3000, 8000, 27017, 6379 | inside the compose networks only | other containers |

The `data` network is `internal`: MongoDB and Redis cannot be reached from outside, and the API
containers cannot reach the internet.

## How load balancing works

nginx has two upstreams, `web:3000` and `api:8000`. Each is declared with `server <name>:<port>
resolve`, a shared memory `zone` and `least_conn`. Docker's DNS returns one address per replica,
nginx re-resolves every 10 seconds (`resolver 127.0.0.11 valid=10s`), and the next request goes
to the replica with the fewest open connections. A replica that fails three times in 10 seconds
(`max_fails=3 fail_timeout=10s`) is skipped, and a failed GET is retried on another replica.
Upstream connections are kept alive. `/api/health` returns the answering container's name in
`instance`, which is how you can see it:

```powershell
1..10 | % { (Invoke-RestMethod http://localhost:8080/api/health).instance } | Sort-Object -Unique
docker compose up -d --scale api=3     # replicas can change while it runs
```

## How caching works

| What | Where | Lifetime | Notes |
|---|---|---|---|
| `/_next/static/*` | nginx | 30 days, `immutable` | file names are content-hashed |
| `/_next/image` | nginx | 1 hour | |
| `/media/*` | nginx | 30 days in nginx, 7 days in the browser | names are not hashed |
| HTML pages | nginx | 10 seconds | `use_stale updating`, `cache_lock`; skipped when the request has a cookie |
| `GET /api/releases*`, `/api/stats` | nginx | 5 seconds | skipped when a cookie is present |
| `/api/download/*` | never | | `Cache-Control: no-store` |
| releases, stats | Redis | 60 s and 30 s | inside the API |

Every cached location answers with `X-Cache-Status` (`HIT`, `MISS`, `STALE`, `UPDATING`,
`BYPASS`). Cache files live in the `nginx_cache` volume. The nginx stats cache plus the API's
Redis cache means a new download can take up to about 35 seconds to show in `/api/stats`.

## Rate limits

Per client address: `/api/` 20 requests per second (burst 40); `/api/download/` 10 requests per
minute (burst 5). Over the limit nginx answers `429` with a JSON body and `Retry-After`. Behind
Docker Desktop every browser on your machine shares one address, which is fine for development.

## How downloads are served

The API validates the version, counts the download, and answers with `X-Accel-Redirect:
/_files/<file>` and `Content-Disposition: attachment`. nginx then serves the file itself from
`/srv/downloads` (sendfile, byte ranges) through an `internal` location. Direct requests for
the installer (`/<file>`, `/downloads/<file>`, `/_files/<file>`) return 404.

## Security headers and logs

Every response carries `X-Content-Type-Options`, `Referrer-Policy`, `Permissions-Policy`,
`X-Frame-Options: DENY` and a Content-Security-Policy that allows only same-origin resources
(inline scripts and styles are allowed for Next.js). `server_tokens` is off, request bodies are
limited to 64 KB. The access log is JSON on stdout (`docker compose logs nginx`), with the client
address, so it is rotated by Docker (3 files of 10 MB). The API never stores an address.

## Tests

| What | Command |
|---|---|
| Back end (pytest, no services needed) | `cd backend; py -3.13 -m venv .venv; .venv\Scripts\pip install -r requirements-dev.txt; .venv\Scripts\python -m pytest` |
| Front end end-to-end (Playwright, needs the stack running) | `cd frontend; npm ci; $env:NODE_TLS_REJECT_UNAUTHORIZED=$null; $env:PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1; npx playwright test` (uses installed Google Chrome; `BASE_URL` defaults to `http://localhost:8080`) |
| Stack smoke test (needs the stack running and a staged release) | `node scripts/smoke.mjs` (`BASE_URL` to point elsewhere) |

The smoke test checks health, that more than one API instance answers, that the home page comes
from the web upstream, a cache `HIT` on a static asset, the installer's SHA-256 against the
manifest, that direct installer paths are 404, that the download is counted once, and that
hammering the download endpoint is rate limited. It takes about a minute (it waits for the stats
cache), and uses a fresh user agent on every run so the once-a-day counting rule does not hide a
failure. Run it at most about once a minute: it uses up the download rate limit.

Validate the configuration without starting anything:

```powershell
docker compose config -q
docker run --rm -v "${PWD}/nginx/nginx.conf:/etc/nginx/nginx.conf:ro" -v "${PWD}/nginx/conf.d:/etc/nginx/conf.d:ro" -v "${PWD}/nginx/snippets:/etc/nginx/snippets:ro" nginx:1.28-alpine nginx -t
```

## Troubleshooting

- **`COUNT_SALT` error from compose**: copy `.env.example` to `.env`.
- **`/api/releases/latest` is 404 and the download button fails**: no release is staged. Run
  `scripts/publish-installer.ps1`, then `docker compose restart api`.
- **nginx stays unhealthy, or 502 / 503**: `docker compose ps` and `docker compose logs web api`.
  nginx waits for `web` and `api` to be healthy; `api` waits for MongoDB and Redis.
- **Port 8080 is taken**: set `SITE_PORT` in `.env`.
- **Only one `instance` ever answers**: check `docker compose ps` shows two `api` containers; a
  replica that is not healthy is skipped.
- **The page looks stale**: HTML is cached for 10 seconds; `X-Cache-Status` shows what happened.
  Bypass with a cookie header, or `docker compose exec nginx sh -c "rm -rf /var/cache/nginx/*/*" && docker compose restart nginx`.
- **Low disk on C:**: Docker's disk is there. Use `docker compose build` only for this project,
  and avoid `docker system prune`, which affects every project on the engine.
- **Changing nginx config**: edit `nginx/`, run the `nginx -t` command above, then
  `docker compose exec nginx nginx -s reload`.
- **Reset the data**: `docker compose down -v` deletes the MongoDB, Redis and cache volumes of
  this project (the download counters start from zero).
