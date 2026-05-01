# Sharepaste — Server + CLI

Self-hosted, end-to-end encrypted clipboard sync. Server only sees ciphertext.

## Quick start

```bash
docker run -d --name sharepaste \
  -v sp-data:/var/lib/sharepaste \
  -p 8443:8443 \
  sharepaste:latest
```

Operate behind a reverse proxy that terminates TLS (Caddy, nginx).

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

The `--db` flag overrides the DB path; default is `/var/lib/sharepaste/sharepaste.sqlite`.

## Wire protocol

See `docs/superpowers/specs/2026-05-01-sharepaste-design.md`. Endpoints:

- `POST /claim-invite`
- `POST /pair/start`, `POST /pair/claim`, `POST /pair/payload`, `GET /pair/payload`, `GET /pair/poll`
- `POST /devices`, `DELETE /devices/:id`
- `POST /entries`, `GET /entries`, `DELETE /entries/:id`, `DELETE /entries`
- `GET /events` (SSE)

All authenticated endpoints take `Authorization: Bearer <device_token>`.

## Tests

```bash
npm test
```

Real Fastify + real SQLite tempfiles. No HTTP mocks.

## Threat model assumptions

- Operator runs HTTPS in front of the container (no in-process TLS in this build).
- Devices use OS disk encryption (FileVault, BitLocker, etc).
- Device-token revocation 401s further requests but does not retroactively un-encrypt entries on a stolen device. Key rotation is out of scope.
