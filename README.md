# sigma-payments

Payment methods (credit card, bank account) for Sigma Tactical Group identity users. Every payment method is owned by exactly one identity `user_id` and tied to one of that user's billing addresses; there is no admin view and no anonymous access — every route requires an active identity session.

This is a **demo payment-method registry, not a PCI-compliant payment processor integration**. It never accepts, logs, or stores a full card number (PAN), CVV/CVC, or full bank account/routing number — only `brand`, `last4` (exactly 4 digits), and (credit cards only) `expiry_month`/`expiry_year`.

Repository: https://github.com/sigmatactical-org/payments

Shared site chrome comes from [sigma-theme](https://github.com/sigmatactical-org/sigma-theme).

## Public vs internal

- **Session-gated web UI** (`payments.sigma-tactical.com`): every route under `/` requires an identity session cookie. Visitors without one are redirected to identity sign-in and returned here afterward. All reads and writes are scoped to the signed-in user's own `user_id` — there is no cross-user or admin view.
- **Outbound only**: this service calls the [addresses](https://github.com/sigmatactical-org/addresses) service's internal JSON API (gated by the shared `SIGMA_INTERNAL_TOKEN`) to list a user's billing addresses and to validate that a submitted `billing_address_id` belongs to the caller. Payments does not itself expose an internal API — nothing currently calls into payments over HTTP.

## Features

- **Credit card and bank account payment methods** — CRUD, one list per identity user
- **Billing address linkage** — every payment method is tied to one of the user's billing addresses, validated over HTTP against the addresses service (not a database foreign key — addresses and payments are independently owned services sharing one Postgres instance)
- **Default payment method** — "Make default" promotes a payment method to the user's default; the database enforces at most one default per `user_id` via a partial unique index, so promoting a new default clears the previous one in the same transaction
- **Strict per-user scoping** — every store method takes the caller's verified `user_id`; a lookup for another user's payment method id returns 404, not 403, so existence can't be probed
- **No sensitive card/account data** — the schema and every form/model in this service have no room for a PAN, CVV, or full bank account/routing number; only `brand`, `last4`, and (credit cards only) expiry

## Configuration

| Variable | Purpose |
|----------|---------|
| `PORT` | Listen port (default `8080`) |
| `DATABASE_URL` | PostgreSQL connection URL (default `postgres://sigma:sigma@127.0.0.1:5432/sigma`) |
| `PAYMENTS_PUBLIC_BASE_URL` | Canonical public URL of this service, for sign-in return links (default `http://127.0.0.1:8090/`) |
| `PAYMENTS_IDENTITY_PUBLIC_URL` | Public identity BFF base URL for the sign-in redirect (default `http://127.0.0.1:3000/`) |
| `PAYMENTS_IDENTITY_INTERNAL_URL` | Cluster-internal identity BFF base URL for server-to-server session checks (falls back to `PAYMENTS_IDENTITY_PUBLIC_URL`) |
| `PAYMENTS_CONTACT_PUBLIC_URL` | Public contact service URL for the navbar link (default `http://127.0.0.1:8083/`) |
| `PAYMENTS_CART_PUBLIC_URL` | Public cart service URL for the navbar link (default `http://127.0.0.1:8084/`) |
| `PAYMENTS_ADDRESSES_INTERNAL_URL` | Cluster-internal addresses service base URL for server-to-server billing-address lookups (default `http://127.0.0.1:8089/`) |
| `PAYMENTS_ADDRESSES_PUBLIC_URL` | Public addresses service URL, for the "add a billing address first" link (default `http://127.0.0.1:8089/`) |
| `SIGMA_INTERNAL_TOKEN` | Shared secret for calling the addresses service's internal JSON API (see [sigma-pg](https://github.com/sigmatactical-org/sigma-pg)) |

## Data model

Each payment method has:

- `user_id` — identity user id (owner; every read/write is scoped to this)
- `method_type` — `credit_card` or `bank_account`, fixed at creation
- `billing_address_id` — id of one of the user's billing addresses (validated over HTTP against addresses, not a DB foreign key)
- optional `label`, `brand`
- `last4` — exactly 4 digits, required
- `expiry_month`, `expiry_year` — required for `credit_card`, must be absent for `bank_account`
- `is_default` — at most one `true` per `user_id`, enforced by a partial unique index

Data lives in the shared PostgreSQL `payments` schema (`payments.payment_methods`), owned by the `payments` role. Schema and role are provisioned by [sigma-pg](https://github.com/sigmatactical-org/sigma-pg)'s migrations, not by this service.

## Admin + JSON API

There is no admin web UI — the web UI at `/` *is* the end-user UI, scoped to whoever is signed in. There is also no internal JSON API exposed by this service (payments is a consumer of addresses' internal API, not a provider of its own).

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | List the signed-in user's payment methods |
| `GET` | `/new` | New payment method form (billing address dropdown populated from addresses) |
| `POST` | `/` | Create a payment method (validates the billing address against addresses first) |
| `GET` | `/{id}/edit` | Edit form (method type is fixed and shown read-only) |
| `POST` | `/{id}` | Update a payment method (re-validates the billing address) |
| `POST` | `/{id}/delete` | Delete a payment method |
| `POST` | `/{id}/default` | Promote to the user's default payment method |

### Behind sigma-identity

Every web route requires an identity session cookie; visitors without one are redirected to:

```
{PAYMENTS_IDENTITY_PUBLIC_URL}/auth/login?app_uri=...&redirect_uri=...
```

and returned to the page they started on after signing in.

## Development

Standalone clone:

```bash
./scripts/prepare-local.sh
cargo run -p sigma-payments
```

Under the sigma workspace (`sigma/it/payments`):

```bash
cd sigma/it/payments && ./scripts/prepare-local.sh && cargo run -p sigma-payments
```

Open http://localhost:8080

## Docker

Release is in **`.github/workflows/release.yml`** when configured. Locally:

```bash
./scripts/docker-build.sh
docker build -f Dockerfile -t sigma-payments:local build/image
```

Data is stored in the shared PostgreSQL `payments` schema (`payments.payment_methods`). Postgres runs in the [platform](https://github.com/sigmatactical-org/platform) kind stack — port-forward for local `cargo run`:

```bash
cd platform && ./scripts/postgres-dev.sh port-forward-bg && ./scripts/postgres-dev.sh migrate
```

## Brand & artwork

© Sigma Tactical Group. **All rights reserved.**

The Sigma Tactical Group name, logos, marks, artwork, and visual identity are **proprietary**. They are not covered by this repository's source-code license. See [BRANDING.md](BRANDING.md).

## License

MIT OR Apache-2.0 for **source code** only. Branding remains proprietary.
