# Sharepaste — Server + CLI

Self-hosted, end-to-end encrypted clipboard sync. Server only sees ciphertext.

## Layout

```
server/        # Node server + operator CLI (build context for Docker)
clients/       # Client apps (desktop Tauri app)
db/            # SQLite host volume mounted into the container
docker-compose.yml
```

## Quick start (docker compose)

```bash
docker compose up -d --build
```

Mounts `./db` → `/var/lib/sharepaste` inside the container. SQLite file lives at `./db/db.sqlite`. Operate behind a reverse proxy that terminates TLS (Caddy, nginx).

## Local dev (no docker)

```bash
cd server
npm install
DB_PATH=../db/db.sqlite npm start -- serve
```

`DB_PATH` defaults to `/var/lib/sharepaste/sharepaste.sqlite` (the container path), so set it explicitly for local dev.

## Operator CLI

Run inside the container:

```bash
# Create a user, get a one-time invite token
docker exec sharepaste sharepaste user create alice

# List users
docker exec sharepaste sharepaste user list

# Revoke a stolen device
docker exec sharepaste sharepaste device revoke <device_id>

# Purge a user's history
docker exec sharepaste sharepaste entry purge --user <user_id>
```

The `--db` flag overrides the DB path; in the compose container it defaults to `/var/lib/sharepaste/db.sqlite` (mounted from `./db`).

## Wire protocol

Endpoints:

- `POST /claim-invite`
- `POST /pair/start`, `POST /pair/claim`, `POST /pair/payload`, `GET /pair/payload`, `GET /pair/poll`
- `POST /devices`, `DELETE /devices/:id`
- `POST /entries`, `GET /entries`, `DELETE /entries/:id`, `DELETE /entries`
- `GET /events` (SSE)

All authenticated endpoints take `Authorization: Bearer <device_token>`.

## Tests

```bash
cd server && npm test
```

Real Fastify + real SQLite tempfiles. No HTTP mocks.

## Threat model assumptions

- Operator runs HTTPS in front of the container (no in-process TLS in this build).
- Devices use OS disk encryption (FileVault, BitLocker, etc).
- Device-token revocation 401s further requests but does not retroactively un-encrypt entries on a stolen device. Key rotation is out of scope.
- The relay is not the only service the desktop app talks to. At launch it asks the **Update Source** (github.com) for the newest release, revealing the machine's address, OS and app version. Nothing about an entry, a key or a relay is transmitted, and the check can be switched off in the app's Settings.
- Downloaded bundles are unsigned and un-notarized. Updates the app fetches for itself are minisign-verified against a public key compiled into the binary; a bundle downloaded from a browser is not verified by anything but Gatekeeper.

See [ADR 0005](docs/adr/0005-unsigned-downloads-signed-updates.md) for why the app contacts a third party at all.
