# crabipie-core

Everything CrabiPie does that isn't drawing pixels. No UI toolkit types.

| Module | What it holds | Original location in main.rs |
|---|---|---|
| `types` | RequestType, HttpMethod, ContentType, AuthType, FormField, ... | 7024-7325 |
| `auth` | AuthRefreshConfig, JSON-path token extraction, `perform_auth_refresh` | 7327-7830 |
| `http` | HttpResponse, QueryParam, RequestHeaders, shared `HTTP_CLIENT` | 7832-7931 |
| `request` | `resolve_vars`, `build_request`, refresh decisions, `merge_set_cookies` | `build_request`, `resolve_vars`, TokenRefreshed/ResponseReceived logic |
| `engine` | `execute_request` (callback-based: buffered / streamed / binary) | `send_request_now` |
| `ws`, `ws_types` | `websocket_stream` -> `Stream<Item = WsEvent>`, `WsConnection` | 6877-7022, 7933-8019 |
| `persist` | SessionState, SavedState (gzip per-tab files), TabMetadata, `state_dir` | 2107-2301, 8021-8062 |
| `env`, `cookies`, `collection` | Environment/EnvVar, cookie parsing, collection tree + ops | 8123-8350, 1518-1643 |
| `graphql` | schema types, parse, selection-set / variables generation | 8351-8476, 8677-8812 |
| `vars`, `constants`, `util` | dynamic vars, templates, find/replace, query-string sync | various |

On-disk formats are unchanged (`~/.crabipie`), so existing tabs/collections load as-is.

Decision: a tab's request config *is* `SavedState`. The native shell keeps one
per tab (plus its own runtime state) and passes it to `build_request`.

Run tests: `cargo test -p crabipie-core`
