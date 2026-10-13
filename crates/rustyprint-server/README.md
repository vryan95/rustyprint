# rustyprint-server

The HTTP service: rustyprint's renderer behind one endpoint, `POST /v1/pdf`. Each release publishes it as a container image for `linux/amd64` and `linux/arm64`:

```sh
docker run -p 8080:8080 ghcr.io/vryan95/rustyprint-server:latest
```

Or run it from the repository with `cargo run --release -p rustyprint-server`. To build the image yourself, see the comment at the top of [docker/server.Dockerfile](../../docker/server.Dockerfile).

| Route | What it does |
| --- | --- |
| `POST /v1/pdf` | Renders a document to PDF |
| `GET /healthz` | Answers `200 OK` with the body `ok` while the server is up |
| `GET /metrics` | Prometheus metrics |

Any other path answers `404`.

## `POST /v1/pdf`

The request is JSON or `multipart/form-data`, chosen by its `Content-Type`. Anything else answers `415`.

### As JSON

`Content-Type: application/json`, with an object:

| Field | Type | |
| --- | --- | --- |
| `html` | string | The document. Required. |
| `options` | object | The render options, as in [render-options.schema.json](../../schema/render-options.schema.json). Optional. |

Any other field is an error. A JSON request carries no files: the document can use the server's shared files (`RUSTYPRINT_ASSETS_DIR`) and the hosts it allows. To send files with the document, use multipart.

```sh
curl -H 'Content-Type: application/json' \
  -d '{"html": "<h1>Hello</h1>", "options": {"page_size": "A4", "margin": "15mm"}}' \
  -o hello.pdf http://localhost:8080/v1/pdf
```

### As multipart

`Content-Type: multipart/form-data`, with these parts, each named by its form field name, or by its file name when it has no field name:

| Part | |
| --- | --- |
| `index.html` | The document, in UTF-8. Required. |
| `options` | The render options, as JSON. Optional. |
| any other name | A file the document refers to, by the path it uses for it without a leading slash, such as `img/logo.png` for `<img src="img/logo.png">` or `/img/logo.png` |

A part without a name is an error.

```sh
curl -F index.html=@invoice.html -F img/logo.png=@brand/logo.png \
  -F 'options={"page_size": "Letter"};type=application/json' \
  -o invoice.pdf http://localhost:8080/v1/pdf
```

### Options and the server's limits

The options are those of [render-options.schema.json](../../schema/render-options.schema.json), the same as the CLI's `--options` and the Ruby and .NET packages take. The server's configuration decides what a request may ask for:

- **`limits`:** each limit the request leaves out is the server's (`RUSTYPRINT_LIMITS`). A request may lower a limit but not raise it: each limit is the lower of the request's and the server's, and a request's `"timeout_ms": null` does not lift the server's deadline.
- **`remote`:** every host in `allowed_hosts` must be one the server allows (`RUSTYPRINT_ALLOWED_HOSTS`): one listed, or a subdomain of a listed `*.example.com`. A request may ask for `*.example.com` only if the server lists exactly that. `allow_http` needs `RUSTYPRINT_ALLOW_HTTP=true`. The fetch limits (`timeout_ms`, `max_bytes`, `max_redirects`, `max_fetches`) are at most their defaults. Without `remote`, nothing is fetched.
- **Files:** relative and root-relative URLs resolve against the request's own files first, then the server's shared files (`RUSTYPRINT_ASSETS_DIR`).

Up to `RUSTYPRINT_CONCURRENCY` renders run at once; other requests wait for one to finish.

### The response

`200 OK` with the PDF as the body, and these headers:

| Header | |
| --- | --- |
| `Content-Type` | `application/pdf` |
| `X-Rustyprint-Warning-Count` | How many warnings the render had, such as `0` |
| `X-Rustyprint-Warnings` | The warnings, problems found in the document's CSS and assets, as a JSON array of strings, such as `` ["invoice.css:12: unsupported property `colr`"] ``. Only when there are warnings. It holds at most the first 50, each cut to 300 characters, with every character outside ASCII escaped as `\uXXXX`. |

### Errors

An error answers with a JSON body, `{"error": "limit_exceeded", "message": "page count exceeds the limit of 1"}`. `error` is one of these, and `message` says what went wrong, for people rather than programs:

| Status | `error` | When |
| --- | --- | --- |
| 400 | `invalid_request` | The body is not valid JSON or multipart, a JSON field is missing or unknown, `index.html` is missing or not UTF-8, or a part has no name |
| 400 | `invalid_options` | The options are not valid, or ask for a host or for plain `http` the server does not allow |
| 413 | `payload_too_large` | The body is larger than `RUSTYPRINT_MAX_BODY_BYTES` |
| 415 | `unsupported_media_type` | The body is neither `application/json` nor `multipart/form-data` |
| 422 | `limit_exceeded` | The document exceeds a limit |
| 500 | `internal` | A bug in rustyprint |
| 503 | `unavailable` | The server is shutting down |
| 504 | `timeout` | Rendering took longer than the deadline |

## `GET /metrics`

Prometheus metrics, as `text/plain; version=0.0.4`:

| Metric | Type | |
| --- | --- | --- |
| `rustyprint_requests_total` | counter | Requests answered, by `status`, on every route |
| `rustyprint_errors_total` | counter | Failed render requests, by `kind`, the `error` of the response |
| `rustyprint_render_seconds` | histogram | Time spent rendering, with buckets from 0.05 to 60 seconds |
| `rustyprint_renders_in_flight` | gauge | Renders running now |

## Configuration

Environment variables configure the server, and set what requests may do. An invalid value stops the server at start, naming the variable.

| Variable | Default | What it sets |
| --- | --- | --- |
| `RUSTYPRINT_ADDR` | `0.0.0.0:8080` | The address to listen on |
| `RUSTYPRINT_CONCURRENCY` | the number of CPUs | How many renders run at once; other requests wait |
| `RUSTYPRINT_MAX_BODY_BYTES` | `52428800` (50 MB) | The largest request body |
| `RUSTYPRINT_ALLOWED_HOSTS` | none | Hosts requests may fetch images and stylesheets from, comma-separated, such as `cdn.example.com,*.example.org` |
| `RUSTYPRINT_ALLOW_HTTP` | `false` | Whether requests may fetch plain `http` URLs from those hosts |
| `RUSTYPRINT_ASSETS_DIR` | none | A directory of shared files, such as fonts and logos, that relative and root-relative URLs resolve against |
| `RUSTYPRINT_LIMITS` | the defaults of the `limits` option | The highest limits, as the `limits` JSON option: `{"max_pages": 200, "timeout_ms": 30000}` |
| `RUST_LOG` | `info` | How much to log, such as `debug` or `rustyprint_server=debug` |

The server logs each request with its method, path, status and time. On `SIGTERM` or Ctrl+C it stops taking requests and finishes the ones in progress.
