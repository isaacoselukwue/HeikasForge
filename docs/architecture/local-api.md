# Local API

## Binding

The server binds to `127.0.0.1` on an available port. Binding to every interface requires an explicit development flag that is hidden from ordinary help output.

## Session establishment

Each server start generates a random bootstrap token held only in memory. `heikas ui` prints it in the URL fragment, so it never appears in a request path or a server log. The interface posts it to `POST /api/v1/session`, which issues an HTTP-only same-site session cookie and a separate readable cross-site token cookie.

`GET /api/v1/session` resumes an existing session so that navigation and reloads do not require the bootstrap token again. It returns unauthorised when no valid session cookie is present.

Token comparison is constant time.

## Request guard

Every route outside health, session and the static graph definition requires a valid session. A state-changing method additionally requires the cross-site token header to match the session, and an origin or referrer that equals the server's own origin. State-changing requests are rate limited per session over a rolling window.

Rejections are typed: unauthorised for a missing or expired session, forbidden for a token or origin mismatch, and too many requests when the rate limit trips.

## Boundary validation

Run identifiers, candidate identifiers and artefact digests are parsed into their domain types at the boundary. An artefact is resolved through a per-run content-addressed index, so no request can name a filesystem path. Range requests are capped. Request bodies are capped, and the task and plan bodies have their own explicit limits.

An unmatched path under `/api` returns a typed not-found document rather than the interface shell, so a client never mistakes a routing error for an empty result.

## Static delivery

Interface assets are embedded in the binary. The static handler serves only from that bundle and never touches the filesystem, so a traversal attempt returns the interface shell rather than a host file. Responses carry a strict content security policy with no inline scripts, plus nosniff, no-referrer and frame denial.

## Event streaming

`GET /api/v1/runs/{id}/stream` is Server-Sent Events. It first replays committed events after the sequence the client supplies, then switches to the live broadcast, so a reconnecting client cannot silently miss events. Each event carries its sequence as the event identifier. When the broadcast lags, the stream emits an explicit lag notice instructing the client to reconnect from its last sequence.

## Parity

The CLI and the API call the same application services. Neither embeds domain behaviour of its own.
