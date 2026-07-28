# nude

**Read-only Docker introspection for Nushell** — a [nu-plugin](https://www.nushell.sh/book/plugins.html) (`nu` + `docker`) that talks straight to the daemon and hands you **typed, structured values** instead of text.

```nu
# stopped containers that exited with an error — as data, not a --format template
nude container ls -a | where state == "exited" and exit_code != 0 | select name exit_code
```

Every command is a `GET`: nude lists, inspects, and searches. It never creates, starts, stops, or removes anything.

---

## Why?

Interacting with docker from a shell looks too much like this:

```nu
# which stopped containers exited non-zero? templates, then text-wrangling
docker ps -a --format '{{.Names}}\t{{.Status}}'
| lines | parse '{name}\t{status}'
| where status =~ 'Exited \([^0]'
```

I wish I could just filter containers like a table. With nude you can:

```nu
docker ps -a | where state == "exited" and exit_code != 0 | select name exit_code
```

Because nude returns real Nushell values, you `where`, `sort-by`, `sum`, and `flatten` them like any data:

```nu
# space held by dangling images
nude image ls --dangling | get size | math sum

# who publishes host port 5432?
nude container ls -o wide | where {|c| $c.ports | any {|p| $p.host_port == 5432}} | get name
```

- **nude does not reimplement all of docker.** It covers the introspection commands that benefit from structured data — the rest stays with the `docker` CLI.
- **nude speaks the Engine API directly** through [bollard](https://crates.io/crates/bollard) (pure Rust). No `docker` CLI, no `socat`, no helper processes — it connects over the local socket (or Windows named pipe), honoring `$DOCKER_HOST`.
- **nude types every field**, so a `datetime` sorts, a `filesize` sums, and docker's human `"Exited (0) 2 hours ago"` becomes a `state` enum, an `exit_code` int, and a `health` string.

## Installation

Build from source (needs **Nushell 0.114+**, **Rust 1.85+**, and a reachable Docker daemon):

```nu
git clone git@github.com:lassoColombo/nude.git
cd nude
cargo build --release

plugin add target/release/nu_plugin_nude
plugin use nude

nude container ls   # verify
```

Add `plugin use nude` to your `config.nu` to load it every session. Re-run `plugin add …` after each rebuild.

## Output modes

Every command takes `--output` (`-o`) with three densities:

| Mode | What you get |
| --- | --- |
| `compact` | Flat rows of **primitive** cells only — built to `where`/`sort-by`/`select`. **Default when listing.** |
| `wide` | Richer; keeps nested lists/records (ports, mounts, IPAM, …). **Default for a single object.** |
| `full` | The raw daemon payload, converted verbatim. |

```nu
nude container ls                      # compact table   (list default)
nude container ls -o wide              # + ports / mounts / networks / …
nude container inspect redis           # wide record     (single-object default)
nude container inspect redis -o full   # the raw daemon payload
```

`ls` lists a resource (`-o wide`/`full` fan an inspect out over every row); `inspect` targets exactly **one** object by name/ID. For the uncurated singletons (`system info`/`version`) `wide == full`; for the flat views (`diff`, `top`, `history`, `search`) `compact == wide`.

## Filtering & completion

Docker's repeatable `-f status=running -f health=healthy` can't be expressed in Nushell, so nude exposes **each filter key as its own typed, tab-completing flag**:

```nu
nude container ls --status running --network bridge --ancestor nginx
nude network ls --scope local --driver bridge
nude image search postgres --official --stars 100
nude image ls --dangling --labels app=web,tier=db     # labels: one comma-separated flag
```

Completion is **live against your daemon**: `--status <TAB>` offers the state enum, `inspect <TAB>` your container names, `--ancestor <TAB>` your images, `--network <TAB>` your networks, and `--labels <TAB>` cycles `key=` → known values → comma. Resources with a label map also take `--show-labels` to add a `labels` column.

## Commands

| Command | Does |
| --- | --- |
| `nude container ls` | List containers |
| `nude container inspect` | Inspect a container by name/ID |
| `nude container diff` | Filesystem changes since the container started |
| `nude container top` | Running processes (like `docker top`) |
| `nude image ls` | List images (one row per `repo:tag`) |
| `nude image inspect` | Inspect an image by name/ID |
| `nude image history` | Layer history (like `docker history`) |
| `nude image search` | Search Docker Hub |
| `nude network ls` | List networks |
| `nude network inspect` | Inspect a network by name/ID |
| `nude volume ls` | List volumes |
| `nude volume inspect` | Inspect a volume by name |
| `nude plugin ls` | List managed plugins |
| `nude plugin inspect` | Inspect a managed plugin |
| `nude system info` | Daemon info: versions, counts, driver, host resources |
| `nude system version` | Daemon version: engine, API, Go, OS/arch, components |

Run `nude <command> --help` for the flags of any command, or expand the [full flag reference](#full-flag-reference) below.

## Recipes

```nu
# stopped containers that exited non-zero
nude container ls -a | where state == "exited" and exit_code != 0 | select name exit_code

# every published port mapping, flattened into one table
nude container ls -o wide
| each {|c| $c.ports | each {|p| {container: $c.name, host: $p.host_port, target: $p.container_port, proto: $p.proto}}}
| flatten

# five largest images
nude image ls | sort-by size --reverse | first 5 | select repository tag size

# total on-disk size of an image's layers
nude image history nginx | get size | math sum

# volumes no container is using
nude volume ls --dangling | get name

# processes in a container — pid and command only
nude container top redis | select pid cmd

# the daemon's engine version, in one field
nude system version | get version

# want docker's native output? just call the binary
^docker ps
```

## Status & roadmap

`container`, `image`, `network`, and `volume` are the fully-fleshed resources; `system` and `plugin` round out daemon and plugin introspection.

- **Implemented** — `container` (`ls`/`inspect`/`diff`/`top`), `image` (`ls`/`inspect`/`history`/`search`), `network` (`ls`/`inspect`), `volume` (`ls`/`inspect`), `plugin` (`ls`/`inspect`), `system` (`info`/`version`).
- **Deferred** — `system df`: bollard 0.21's disk-usage model doesn't match what current daemons return, so it needs a hand-rolled request over the socket.
- **Planned** — the swarm resources (`service`, `node`, `task`, `config`, `secret`) fit nude's list + inspect + filter shape, but their endpoints require a swarm-manager daemon.

---

<details id="full-flag-reference">
<summary><strong>Full flag reference</strong> — every command's parameters and flags (generated from the plugin's own signatures)</summary>

### `nude container diff`

Filesystem changes to a container since it started

**Parameters**

| Parameter | Type     | Description          |
| --------- | -------- | -------------------- |
| `name`    | `string` | Container name or ID |

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude container inspect`

Inspect a container by name/ID

**Parameters**

| Parameter | Type     | Description          |
| --------- | -------- | -------------------- |
| `name`    | `string` | Container name or ID |

**Flags**

| Flag             | Type     | Description                                                              |
| ---------------- | -------- | ------------------------------------------------------------------------ |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide)                              |
| `--show-labels`  | `switch` | Enrich the container with a `labels` column (wide; full always has them) |

### `nude container ls`

List containers

**Flags**

| Flag             | Type     | Description                                                                       |
| ---------------- | -------- | --------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                         |
| `--all`, `-a`    | `switch` | Include stopped containers                                                        |
| `--show-labels`  | `switch` | Enrich each container with a `labels` column (compact/wide; full always has them) |
| `--status`       | `string` | State: created\|restarting\|running\|removing\|paused\|exited\|dead               |
| `--health`       | `string` | Health: starting\|healthy\|unhealthy\|none                                        |
| `--exited`       | `int`    | Exit code (matches stopped containers)                                            |
| `--ancestor`     | `string` | Created from this image (name[:tag], id, or digest)                               |
| `--before`       | `string` | Created before this container (name or id)                                        |
| `--since`        | `string` | Created since this container (name or id)                                         |
| `--name`         | `string` | Name substring (use `inspect` for an exact lookup)                                |
| `--id`           | `string` | Container id prefix                                                               |
| `--network`      | `string` | Attached to this network (name or id)                                             |
| `--volume`       | `string` | Uses this volume (name or mount destination)                                      |
| `--publish`      | `string` | Publishes this port (`port[/proto]` or `start-end[/proto]`)                       |
| `--expose`       | `string` | Exposes this port (`port[/proto]` or `start-end[/proto]`)                         |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                    |
| `--is-task`      | `switch` | Only swarm service tasks                                                          |

### `nude container top`

Running processes in a container (like `docker top`)

**Parameters**

| Parameter | Type     | Description                            |
| --------- | -------- | -------------------------------------- |
| `name`    | `string` | Container name or ID (must be running) |

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude image history`

Layer history of an image (like `docker history`)

**Parameters**

| Parameter | Type     | Description                      |
| --------- | -------- | -------------------------------- |
| `name`    | `string` | Image reference (repo:tag) or ID |

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude image inspect`

Inspect an image by name/ID

**Parameters**

| Parameter | Type     | Description                      |
| --------- | -------- | -------------------------------- |
| `name`    | `string` | Image reference (repo:tag) or ID |

**Flags**

| Flag             | Type     | Description                                                          |
| ---------------- | -------- | -------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide)                          |
| `--show-labels`  | `switch` | Enrich the image with a `labels` column (wide; full always has them) |

### `nude image ls`

List images

**Flags**

| Flag             | Type     | Description                                                                   |
| ---------------- | -------- | ----------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                     |
| `--all`, `-a`    | `switch` | Include intermediate (layer) images                                           |
| `--show-labels`  | `switch` | Enrich each image with a `labels` column (compact/wide; full always has them) |
| `--reference`    | `string` | Reference pattern (name[:tag], e.g. `nginx` or `ngin*`)                       |
| `--before`       | `string` | Created before this image (name or id)                                        |
| `--since`        | `string` | Created since this image (name or id)                                         |
| `--dangling`     | `switch` | Only dangling (untagged) images                                               |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                |

### `nude image search`

Search Docker Hub for images

**Parameters**

| Parameter | Type     | Description                                                         |
| --------- | -------- | ------------------------------------------------------------------- |
| `term`    | `string` | Search term (matched against Docker Hub image names & descriptions) |

**Flags**

| Flag             | Type     | Description                                                    |
| ---------------- | -------- | -------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)      |
| `--limit`        | `int`    | Maximum number of results (Docker Hub caps at 100; default 25) |
| `--stars`        | `int`    | Only images with at least this many stars                      |
| `--official`     | `switch` | Only official images (disabled-only: `\| where not official`)  |

### `nude network inspect`

Inspect a network by name/ID

**Parameters**

| Parameter | Type     | Description        |
| --------- | -------- | ------------------ |
| `name`    | `string` | Network name or ID |

**Flags**

| Flag             | Type     | Description                                                            |
| ---------------- | -------- | ---------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide)                            |
| `--show-labels`  | `switch` | Enrich the network with a `labels` column (wide; full always has them) |

### `nude network ls`

List networks

**Flags**

| Flag             | Type     | Description                                                                     |
| ---------------- | -------- | ------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                       |
| `--show-labels`  | `switch` | Enrich each network with a `labels` column (compact/wide; full always has them) |
| `--driver`       | `string` | Network driver: bridge\|host\|overlay\|macvlan\|ipvlan\|none                    |
| `--id`           | `string` | Network id prefix                                                               |
| `--name`         | `string` | Name substring (use `inspect` for an exact lookup)                              |
| `--scope`        | `string` | Scope: local\|global\|swarm                                                     |
| `--type`         | `string` | Type: builtin\|custom                                                           |
| `--dangling`     | `switch` | Only networks not used by any container                                         |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                  |

### `nude plugin inspect`

Inspect a managed plugin by name

**Parameters**

| Parameter | Type     | Description |
| --------- | -------- | ----------- |
| `name`    | `string` | Plugin name |

**Flags**

| Flag             | Type     | Description                                 |
| ---------------- | -------- | ------------------------------------------- |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide) |

### `nude plugin ls`

List managed plugins

**Flags**

| Flag             | Type     | Description                                                                             |
| ---------------- | -------- | --------------------------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                               |
| `--capability`   | `string` | Capability: volumedriver\|networkdriver\|ipamdriver\|authz\|logdriver\|metricscollector |
| `--enabled`      | `switch` | Only enabled plugins                                                                    |

### `nude system info`

Daemon-wide info: versions, counts, driver, host resources

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude system version`

Daemon version: engine, API, Go, OS/arch, and components

**Flags**

| Flag             | Type     | Description                                               |
| ---------------- | -------- | --------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact) |

### `nude volume inspect`

Inspect a volume by name

**Parameters**

| Parameter | Type     | Description |
| --------- | -------- | ----------- |
| `name`    | `string` | Volume name |

**Flags**

| Flag             | Type     | Description                                                           |
| ---------------- | -------- | --------------------------------------------------------------------- |
| `--output`, `-o` | `string` | Output format: wide \| full (default: wide)                           |
| `--show-labels`  | `switch` | Enrich the volume with a `labels` column (wide; full always has them) |

### `nude volume ls`

List volumes

**Flags**

| Flag             | Type     | Description                                                                    |
| ---------------- | -------- | ------------------------------------------------------------------------------ |
| `--output`, `-o` | `string` | Output format: compact \| wide \| full (default: compact)                      |
| `--show-labels`  | `switch` | Enrich each volume with a `labels` column (compact/wide; full always has them) |
| `--driver`       | `string` | Volume driver (e.g. `local`, or a volume plugin)                               |
| `--name`         | `string` | Name substring (use `inspect` for an exact lookup)                             |
| `--dangling`     | `switch` | Only volumes not used by any container                                         |
| `--labels`       | `string` | Labels, comma-separated: `key` or `key=value` (e.g. `a=b,c=d`)                 |

</details>
