# Slate suggestion intake

The Slate website and app can send text suggestions anonymously to the same HTTPS endpoint. The Worker writes one JSON file per suggestion under `submissions/YYYY/MM/` in an owner-controlled private GitHub repository. Visitors receive a receipt ID. The Worker exposes no read route and records neither client IP addresses nor suggestion text in its logs.

## Configure GitHub

1. Choose a personally owned private repository with an initialized default branch. The sample uses `jmmoser7/slate-agent-feedback`. Check the repository's access settings for pending invitations. Use a dedicated private repo if existing collaborators should keep their access to the current repo.
2. Create an owner-only [GitHub App](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/registering-a-github-app). Grant repository **Contents: Read and write** with the required **Metadata: Read**. Disable webhooks and install the App on only the destination repository.
3. In the local deployment config, set the owner, repository name, numeric repository ID, App **Client ID**, and installation ID. The numeric repo ID is the `id` in the [repository metadata response](https://docs.github.com/en/rest/repos/repos#get-a-repository). Find the installation ID in its GitHub settings URL.
4. Generate the [GitHub App private key](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/managing-private-keys-for-github-apps). If its PEM header says `BEGIN RSA PRIVATE KEY`, convert to PKCS#8 locally. Keep both files outside the checkout.

   ```text
   openssl pkcs8 -topk8 -nocrypt -in app-private-key.pem -out app-pkcs8.pem
   ```

## Deploy the Worker

Use an owner-controlled Cloudflare Workers account and Node.js 22.12 or newer for Wrangler. Fill the blank non-secret values in a local copy of `wrangler.toml`. Keep that file beside `worker.mjs`. If the rate namespace ID is already in use in that account, choose a fresh value. The sample limit is eight requests per minute per IP, per Cloudflare location.

```powershell
node --test services/suggestion-intake/worker.test.mjs
npx wrangler@4 login
npx wrangler@4 secret put GITHUB_APP_PRIVATE_KEY --config services/suggestion-intake/wrangler.local.toml
npx wrangler@4 deploy --config services/suggestion-intake/wrangler.local.toml
```

At the secret prompt, paste the complete PKCS#8 PEM through the terminal. Cloudflare may prompt to create the Worker on the first secret upload. Use the public HTTPS deployment URL with `/suggestions` for both callers. Browser requests must come from `BROWSER_ORIGIN`, which is the Pages origin without a path or trailing slash. Desktop requests omit `Origin`.

Send a synthetic test record from the public website without a GitHub sign-in and verify it appears only in the private repo. Resubmit the same payload to check for a single file and a successful replay receipt. Change the test text while retaining the ID to check conflict handling. Keep the repository private during operation.

## Request and receipt

`POST /suggestions` accepts an application/json object with `id`, `source`, `kind`, `title`, `body`, and optional `version`. Source is `website` or `slate`. Kind is `Bug`, `Feature`, or `Other`. Maximum lengths are 160 for title, 6,000 for body, and 64 for version. The full request is limited to 32 KiB.

Create the ID once per draft submission as `YYYYMM-<lowercase UUIDv4>` using the UTC month. Keep it on retry, including after month rollover. Use a fresh ID for a changed payload. `201` is saved and `200` is an identical replay. Both return `{"id":"…"}`. Errors use `{"error":"…"}`.

| Status | Error |
|---|---|
| 400 | `invalid_submission` |
| 403 | `invalid_origin` |
| 404 | `not_found` |
| 405 | `method_not_allowed` |
| 409 | `id_conflict` |
| 429 | `rate_limited` |
| 503 | `submission_unavailable` |

The Worker mints a short-lived installation token scoped to the configured repository. Before each write it verifies the repository identity, private visibility, and personal ownership, then checks that the complete GitHub collaborator list contains no one besides the owner. An unavailable, unexpected, or paginated access response returns `503`. A successful new commit or a verified matching retry is required for a receipt. The unit tests use a generated key and mock GitHub responses. A deployed smoke test is still required for account configuration.
