---
name: publish
description: Publish things for other people to open with a link. Beacon (beacon.host) hosts static reports, files and media. Lakebed (lakebed.dev) hosts small apps with a database, live updates and Google sign-in, such as review or feedback pages. Use when the user asks to share, host or publish a report, file, page or small app, or names Beacon or Lakebed.
---

# Publish

## Which one

- **Beacon** for anything static: an HTML report, screenshots, a folder of
  files, videos. Files get a public URL, CORS `*`, range requests and long
  caching. No server code or database on the free plan.
- **Lakebed** when visitors must save something or see each other's changes:
  feedback forms, review pages, votes, small tools with state. It is a hosted
  Preact client plus server functions and a database, in public alpha.
- **Both** when an app needs large media. Lakebed only stores 1 MiB of rows and
  ships a 1 MiB bundle, so put videos and images on Beacon and load them from
  the Lakebed app by URL.

Both publish to the user's own accounts. Keep keys and tokens out of chat and
project files.

## Beacon

Publish with curl and `BEACON_API_KEY`, stored in
`~/.config/environment.d/55-secrets.conf`. If this process predates the
environment update, source that file in the Bash subprocess. A missing key must
fail instead of publishing anonymously.

For a single HTML report (Bash):

```bash
set -o pipefail
curl --fail-with-body --silent --show-error https://beacon.host/v1/publish \
  --header "Authorization: Bearer ${BEACON_API_KEY:?BEACON_API_KEY is not set}" \
  --form 'file=@report.html;filename=index.html;type=text/html' \
  | jq -er '.fileUrl // .url'
```

For a report containing `<img src="images/before.png">` and
`<img src="images/after.png">`, publish the HTML and both images in one call:

```bash
set -o pipefail
curl --fail-with-body --silent --show-error https://beacon.host/v1/publish \
  --header "Authorization: Bearer ${BEACON_API_KEY:?BEACON_API_KEY is not set}" \
  --form 'file=@report.html;filename=index.html;type=text/html' \
  --form 'file=@images/before.png;filename=images/before.png;type=image/png' \
  --form 'file=@images/after.png;filename=images/after.png;type=image/png' \
  | jq -er '.fileUrl // .url'
```

- Keep absolute HTTPS image links as they are. For local images, add a form
  field for each file, preserving the path referenced in the HTML:
  `--form 'file=@images/chart.png;filename=images/chart.png'`.
- Add `--form 'slug=report-name'` to choose a URL or update an owned report at
  the same URL. Updates replace the full file set, so resend its local images.
- Add `--form 'expires=2h'` for a temporary report; otherwise account-owned
  reports have no expiry.
- Fetch the returned URL and check local images before reporting success.
  Give the user the URL and any expiry they requested.

That endpoint accepts up to 10 MB and 1,000 files per call. For anything
larger, such as videos, run `scripts/beacon_upload.py DIR SLUG` from this
skill's directory. It hashes every file, creates the site or a new version, and
uploads only files Beacon doesn't already have. A new version replaces the
whole file set, so pass the complete folder each time. The site is served at
`https://SLUG.beacon.host/<path>`.

- Put 8 or more hex characters of each file's hash in its name (for example
  `clip.b1373393c3.mp4`). Beacon then serves it as immutable, and a changed file
  gets a new URL, so no browser shows a stale copy.
- MP4s must have `moov` before `mdat` (`ffmpeg -movflags +faststart`) so they
  start playing before they finish downloading.
- The free plan holds 1 GB of files in total. Check what you're about to upload
  and tell the user if it's close.

For other API operations, read the relevant section of
https://beacon.host/llms-full.txt or https://beacon.host/openapi.json.

## Lakebed

The CLI is logged in as the user (GitHub sign-in); check with
`pnpm dlx lakebed auth status` (`npx lakebed` works too). Run every command
from the app directory.

- `pnpm dlx lakebed new NAME` creates an app in a new directory. Read its
  generated `AGENTS.md` before writing code; it is the current API reference.
- `pnpm dlx lakebed dev --port PORT` runs it locally with real guest sessions
  and Google sign-in. `db dump --port PORT` and `logs --port PORT` read the
  local copy.
- `pnpm dlx lakebed deploy` publishes it as an app owned by the user's account,
  with no expiry, and writes `lakebed.json` with the deploy ID. Deploying again
  from the same directory updates the same app and URL; keep `lakebed.json`.
- `pnpm dlx lakebed db dump DEPLOY_ID` reads the hosted database (only the
  owner can). `logs DEPLOY_ID` and `inspect DEPLOY_ID --usage` show logs and
  quota use. `deploy list` lists the user's apps.

How apps work:

- `server/index.ts` exports `capsule({ name, auth, schema, queries, mutations })`.
  `client/index.tsx` exports `App` and calls them with `createClient`,
  `useQuery` and `useMutation`. Queries update live in every open browser.
- No npm packages or Node built-ins. Style with Tailwind classes, inline styles
  or a `<style>` element. Only the favicon is served as a static file.
- Every visitor gets a guest session automatically. Store the owner with
  `userId()` fields and check `ctx.auth.requireIdentity()` in each handler;
  Google sign-in later moves a guest's rows to their account.
- Field names `id`, `createdAt` and `updatedAt` are reserved.
- Every app is public: anyone with the URL can open it, because the alpha
  can't restrict access. Keep private data out, and have the server, not the
  client, decide what each visitor may read or change.
- Limits per app: 1 MiB bundle, 1 MiB of rows, 10,000 requests and 1,000
  writes a day. Debounce saves from text fields and store one row per item
  rather than one per keystroke.

Before reporting success, open the deployed URL, make a change, read it back
with `db dump`, and remove your test rows. Give the user the URL and the
command for reading what visitors submit.

Docs: https://docs.lakebed.dev/llms.txt
