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
`docker compose restart api`. The script also writes the same manifest to
`backend/releases.json`, the copy used on Vercel (see "Deploying on Vercel").

`downloads/` is mounted read-only into nginx and the API at `/srv/downloads`.

## Deploying on Vercel

This is an alternative to docker compose: the same site and API, hosted as **two Vercel projects**
from the one GitHub repository. There is no nginx there, so the installer comes from Vercel Blob
and the API redirects to it. Docker compose keeps working unchanged.

| | Project "API" | Project "Site" |
|---|---|---|
| What | FastAPI as a Python function | Next.js |
| Root Directory | `website/backend` | `website/frontend` |
| Framework preset | FastAPI (detected) | Next.js (detected) |
| Branch | `main` | `main` |

The API project must find a top-level `app` in `app/main.py` (it does, so the
`FASTAPI_ENTRYPOINT_NOT_FOUND` error is gone). `backend/.python-version` pins Python 3.13,
`backend/requirements.txt` is what Vercel installs, and `backend/vercel.json` only sets a 30 s
time limit and leaves the tests out of the bundle. `backend/releases.json` is the release list
the API reads (see step 5).

### Step by step

1. **Create the API project.** In Vercel: Add New > Project, import `Asim1921/Tributary`, set
   **Root Directory** to `website/backend`. Leave the build and install settings empty. Do not
   deploy yet (or let the first deploy fail; it will work after step 3).
2. **Add the databases** (project > Storage > Marketplace, or the Integrations page):
   - **MongoDB Atlas**: create a free cluster and connect it to the API project. It sets
     `MONGODB_URI`. In Atlas under Network Access allow connections from anywhere
     (`0.0.0.0/0`), because Vercel functions have no fixed address; the database user's
     password is what protects it.
   - **Redis** (Upstash Redis or Redis Cloud): connect it to the API project. It sets
     `REDIS_URL` (older Upstash KV integrations set `KV_URL`; both work). `rediss://` (TLS) URLs
     are fine.
3. **Upload the installer to Vercel Blob.** Storage > Create > Blob (a public store), then
   upload `Tributary_0.1.0_x64-setup.exe` (about 5.2 MB) from
   `target/release/bundle/nsis/` (after `npm run tauri build`). Put it in a folder, for example
   `tributary/`, so its URL looks like
   `https://<id>.public.blob.vercel-storage.com/tributary/Tributary_0.1.0_x64-setup.exe`.
   Copy that URL **without the file name**: that folder URL is `DOWNLOAD_BASE_URL`. Any other
   public HTTPS location works the same way. The file name must match the `file` in
   `backend/releases.json`; keep the Blob file name unchanged (no random suffix).
4. **Set the API environment variables** (project > Settings > Environment Variables, for
   Production and Preview):

   | Variable | Value |
   |---|---|
   | `MONGODB_URI` | set by the Atlas integration (or set `MONGO_URL` yourself) |
   | `REDIS_URL` | set by the Redis integration (or `KV_URL`) |
   | `DOWNLOAD_BASE_URL` | the Blob folder URL from step 3, no trailing file name |
   | `COUNT_SALT` | a long random value, see below. Required in practice: the default is public |
   | `MONGO_DB` | optional, default `tributary` |
   | `CACHE_TTL_RELEASES`, `CACHE_TTL_STATS` | optional, default 60 and 30 seconds |
   | `LOG_LEVEL` | optional, default `info` |

   Generate `COUNT_SALT` in PowerShell:
   `-join ((1..48) | % { '{0:x}' -f (Get-Random -Maximum 16) })`
   or with `openssl rand -hex 32`. Never commit it. Changing it later only means people who
   already downloaded today can be counted once more.
5. **Release list.** The API project only sees `website/backend`, so it reads
   `backend/releases.json` (size and SHA-256 come from it; the installer does not need to be on
   the server). `scripts/publish-installer.ps1` writes this copy and `downloads/releases.json`
   together. After publishing a new version: commit `backend/releases.json`, upload the new
   installer to the same Blob folder, push, and the next request on each instance picks it up.
   `RELEASES_FILE` can point somewhere else.
6. **Deploy the API** (Deployments > Redeploy). Check it (see below) and note its address, for
   example `https://tributary-api.vercel.app`.
7. **Create the Site project**: import the same repository again, Root Directory
   `website/frontend`. Set its environment variable(s) as listed in the front end's own notes so
   that `/api/*` reaches the API project's address from step 6, then deploy.
8. Add your domain to the **Site** project only. People use the Site address; the API address is
   internal.

Order: databases and Blob first, then the API, then the Site (it needs the API's address).

### Checking that it worked

Replace `API` with the API project's address (or the Site's, once it forwards `/api`):

```powershell
Invoke-RestMethod https://API/api/health            # status "ok", mongo "ok", redis "ok"
Invoke-RestMethod https://API/api/releases/latest   # version 0.1.0, size, sha256
curl.exe -I https://API/api/download/latest         # 302, Location: <DOWNLOAD_BASE_URL>/Tributary_0.1.0_x64-setup.exe, Cache-Control: no-store
Invoke-RestMethod https://API/api/stats             # downloads.total goes up by one (once per visitor per day)
```

If `/api/health` says `503` and `mongo` or `redis` is `down`, the variable is missing or Atlas
is not allowing the connection. If `/api/releases/latest` is `404`, check that
`backend/releases.json` is committed and valid. If the download is `404`, the version is not in
the manifest.

### Running with no database at all

The site works on Vercel with no MongoDB and no Redis. When neither `MONGO_URL`/`MONGODB_URI`
nor `REDIS_URL`/`KV_URL` is set, the API keeps everything in memory: releases come from the
bundled manifest (`backend/releases.json`), downloads redirect to `DOWNLOAD_BASE_URL/<file>`, and
`/api/health` answers `200` with `"mongo": "not configured"` and `"redis": "not configured"`.
Download counts still work, but they are kept in memory, so they last only as long as one
function instance (they reset on a cold start and are not shared between instances).

Adding **MongoDB Atlas** and **Upstash Redis** from the Vercel Marketplace later (they set
`MONGODB_URI` and `REDIS_URL`/`KV_URL`) turns on lasting, shared download counts with no code
change: redeploy the API project and it picks them up. Each one is independent. docker compose
sets both URLs explicitly, so it is unaffected.

### How the API behaves on Vercel

- No startup wait and no background retry loop: the API connects to MongoDB and Redis on the
  first request that needs them, creates the indexes and loads the releases once per instance
  (idempotent, safe with many instances), and tries again on a later request if that failed.
- `GET /api/download/latest` and `/api/download/<version>` count the download exactly as before
  and answer `302` to `DOWNLOAD_BASE_URL/<file>` with `Cache-Control: no-store`. When
  `DOWNLOAD_BASE_URL` is not set (docker compose), they answer with `X-Accel-Redirect` for nginx.
- Serverless mode is on when `DOWNLOAD_BASE_URL` is set or Vercel's `VERCEL` variable is
  present; `SERVERLESS=true|false` overrides it.

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
