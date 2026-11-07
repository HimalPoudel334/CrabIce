# CrabiPie – module layout

Dependency direction: `ui → app → domain` (domain never imports app or ui).

```
src/
├── main.rs                 entry point only
├── domain/                 data + logic, no Message / app state / widgets
│   ├── http.rs             RequestType, HttpMethod, ContentType, headers, query params, form fields, HTTP_CLIENT
│   ├── auth.rs             auth types + automatic login / token refresh
│   ├── environment.rs      Environment, EnvVar
│   ├── variables.rs        {{var}} regex, dynamic vars ($timestamp, $randomInt, ...)
│   ├── collection.rs       collection tree model, tree operations, disk IO
│   ├── cookies.rs          cookie entries, Set-Cookie parsing
│   ├── graphql.rs          introspection model, schema parsing, query generation
│   ├── websocket.rs        ws message/event types, connect()
│   ├── persistence.rs      session / tab metadata / per-tab saved state on disk
│   ├── view_state.rs       sub-tab selections stored with a saved request
│   └── pdf_viewer.rs
├── app/                    application state and what mutates it
│   ├── mod.rs              CrabiPie struct + new()/title()
│   ├── message.rs          Message enum
│   ├── tab.rs              TabState / TabLoadState
│   ├── tabs.rs find.rs request.rs environment.rs graphql.rs media.rs
│   ├── collections.rs persistence.rs websocket.rs subscription.rs
│   └── update/             update() router + one handler module per message group
└── ui/                     everything that draws
    ├── view.rs             view(), tab bar, title row, overlays
    ├── request_panel.rs response_panel.rs sidebar.rs graphql.rs websocket.rs
    ├── environment_panel.rs cookie_jar.rs find_dialog.rs
    └── theme.rs format.rs json_highlighter.rs
```

Adding a message: add the variant in `app/message.rs`, handle it in the matching
`app/update/<group>.rs`, and list it in the router in `app/update/mod.rs`.
