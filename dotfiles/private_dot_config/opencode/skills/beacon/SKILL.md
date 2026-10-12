---
name: beacon
description: Publish HTML reports and generated artifacts to beacon.host using curl. Use when the user chooses Beacon or asks to share a report or generated file as a public link.
---

# Beacon

Publish with curl and the `BEACON_API_KEY` environment variable, stored in
`~/.config/environment.d/secrets.conf`. If this process predates the environment
update, source that file in the Bash subprocess. Keep the key out of chat and
project files; a missing key must fail instead of publishing anonymously.

For a single HTML report (Bash):

```bash
curl --fail-with-body --silent --show-error https://beacon.host/v1/publish \
  --header "Authorization: Bearer ${BEACON_API_KEY:?BEACON_API_KEY is not set}" \
  --form 'file=@report.html;filename=index.html;type=text/html' \
  | jq -er '.fileUrl // .url'
```

For a report containing `<img src="images/before.png">` and
`<img src="images/after.png">`, publish the HTML and both images in one call:

```bash
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

This endpoint accepts up to 10 MB and 1,000 files per authenticated call.
For larger uploads or other API operations, read the relevant section of
https://beacon.host/llms-full.txt or https://beacon.host/openapi.json.
