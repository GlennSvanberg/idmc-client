# Performance

A Customer 360 or Product 360 org has millions of master records. The client must not download them to draw a table.

## What the table does

The first search returns the hit count and the first window (500 records). The table’s scrollbar is sized to that count immediately, including when the count comes from a cache file written last time. Scrolling asks for the window under the viewport and one window ahead. At most four searches are in flight. A 429 backs off and does not start another.

Sort is a `sort` clause on the search (`fieldName` prefixed the way the Search API expects, `ASCENDING` or `DESCENDING`). Rows stay in the order the server sent. The client does not sort a page again, so a row does not jump when a later page arrives.

A filter box on a master table sends either the text as `search` or, for `field=value`, a `CONTAINS` filter (an empty value is `IS_MISSING`). The Search API rejects a request that sets both `search` and `fields`, so the client does not send `fields`. Column projection is not available on this call.

Child and detail reads stay lazy: a linked table asks for the related rows of the selected parent, and a thin master record is filled with one business-entity GET when a nested group is opened.

## Local cache

Each window is stored under the user cache directory (`IDMC_CACHE`, else `$XDG_CACHE_HOME/idmc-client` or `~/.cache/idmc-client`), keyed by the MDM host, entity, sort, and query. Opening the client paints that snapshot at once and refreshes offset 0 in the background. The file keeps up to 20 windows, not the whole entity.

## What we measured

On 4,000 mock Person rows, building the sorted row array once and then reading row 0 and row 3,000 from that array is the stand-in for “first rows” and “scroll to row 3,000”. The test prints:

```text
timing: first rows …
timing: row 3000 ready …
```

A live session prints the same lines when the first window returns and when the window that contains row 3,000 is stored. The time includes the login that `LiveBackend::open` already performed.

The earlier UI walked every loaded row, sorted by a composed label, and built a GPUI element per row on each frame. That is gone: the list virtualizes, the row array is cached until the generation changes, and a live master list is never sorted on the client.

## Egress snapshot (not what this build does)

Search windows are the right default. They are current, they respect the user’s sort and filter, and they do not require a job. They are not a local index: jumping to an arbitrary row still waits on a search, and Ctrl+K full text is one small search per master entity.

A full local index would use an Egress job instead:

`POST {mdm}/job/public/api/v1/jobInstance/Egress`

The body names the entity (`sourceSystem: businessEntity`), a file target (`targetSystem: File`, csv or an Excel-like flat file), and the field list. The platform writes the file to its storage. The client would then download that file (the job APIs, not the per-record Search API), load it into a local store such as SQLite or a tantivy index, and serve the table and Ctrl+K from disk. A later Egress run replaces the snapshot. That path is the one to add when a windowed search is still too slow for the way people scroll, or when search must stay instant while the network is down. It is a snapshot, so it lags the hub until the next job, and it is a large download. This build does not start Egress jobs.

## Dev builds

`[profile.dev.package."*"]` sets `opt-level = 2` so GPUI and the HTTP stack are not interpreted-slow, while this crate itself still compiles quickly. `cargo run -p idmc-client --release` is what to use for a timed pass on a real org.
