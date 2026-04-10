# SearxNG Setup Guide for Engram

SearxNG is a self-hosted meta search engine that engram uses as its primary web search provider. This guide covers installation, configuration, and integration with engram.

## Quick Start with Docker

```bash
docker run -d \
  --name searxng \
  -p 8090:8080 \
  -v ./searxng:/etc/searxng \
  searxng/searxng:latest
```

SearxNG will be available at `http://localhost:8090`.

## Required Configuration

### Enable JSON Output

Engram requires JSON output from SearxNG. Edit `settings.yml` (mounted at `./searxng/settings.yml`):

```yaml
search:
  formats:
    - html
    - json
```

Without this, engram cannot parse search results.

### Enable Search Engines

By default, SearxNG enables a limited set of engines. For best results with engram, enable additional engines in `settings.yml`:

```yaml
engines:
  - name: google
    engine: google
    disabled: false

  - name: bing
    engine: bing
    disabled: false

  - name: duckduckgo
    engine: duckduckgo
    disabled: false

  - name: wikipedia
    engine: wikipedia
    disabled: false

  - name: wikidata
    engine: wikidata
    disabled: false
```

You can also enable engines through the SearxNG web UI under **Preferences > Engines**.

### Rate Limiting / Delays

SearxNG respects rate limits of upstream engines. If you get blocked or see errors:

1. **Increase request delays** in `settings.yml`:

```yaml
outgoing:
  request_timeout: 5.0
  max_request_timeout: 15.0

# Per-engine rate limiting
engines:
  - name: google
    engine: google
    timeout: 6.0
    shortcut: go
```

2. **Use proxies** for high-volume searches:

```yaml
outgoing:
  proxies:
    all://:
      - http://proxy1:8080
      - http://proxy2:8080
```

3. **Enable limiter** to prevent bans:

```yaml
server:
  limiter: true
```

## Connecting to Engram

### Via System Page

1. Go to **System > Sources & Integrations > Web Search Providers**
2. Click **+ Add Provider**
3. Set:
   - **Name**: Local SearxNG
   - **Provider**: searxng
   - **URL**: `http://localhost:8090` (or your SearxNG address)
4. Test the connection

### Via Onboarding Wizard

Select "SearXNG (Self-hosted)" in the Web Search step and enter your SearxNG URL.

### Via API

```bash
curl -X POST http://localhost:3030/config \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"search_providers": [{"name": "Local SearxNG", "provider": "searxng", "url": "http://localhost:8090", "enabled": true}]}'
```

## Verifying the Setup

Test that SearxNG returns JSON:

```bash
curl "http://localhost:8090/search?q=test&format=json"
```

You should get a JSON response with `results` array. If you get HTML or an error, check that `format: json` is enabled in `settings.yml`.

## Troubleshooting

| Issue | Solution |
|---|---|
| No results | Enable more engines in Preferences > Engines |
| Timeout errors | Increase `request_timeout` in `settings.yml` |
| 429 / rate limited | Enable limiter, add delays, or use proxies |
| HTML instead of JSON | Add `json` to `search.formats` in `settings.yml` |
| Connection refused | Check Docker is running: `docker ps` |
