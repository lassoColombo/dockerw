# docker-wrapper (dockerw)

**Structured, typed, read-only Docker introspection for Nushell.**

`dockerw` overrides the default docker introspection commands with its own implementation.  
You keep typing `docker ps`, but you get back Nushell values instead of text.  

---

- [docker-wrapper (dockerw)](#docker-wrapper-(dockerw))
  - [Why?](#why?)
  - [How the shadowing works](#how-the-shadowing-works)
  - [Installation](#installation)
    - [Requirements](#requirements)
  - [Differences from docekr commands](#differences-from-docekr-commands)
    - [Output modes](#output-modes)
    - [Filtering](#filtering)
  - [Commands](#commands)
    - [`docker container inspect`](#`docker-container-inspect`)
    - [`docker diff`](#`docker-diff`)
    - [`docker history`](#`docker-history`)
    - [`docker image inspect`](#`docker-image-inspect`)
    - [`docker images`](#`docker-images`)
    - [`docker info`](#`docker-info`)
    - [`docker inspect`](#`docker-inspect`)
    - [`docker network inspect`](#`docker-network-inspect`)
    - [`docker network ls`](#`docker-network-ls`)
    - [`docker port`](#`docker-port`)
    - [`docker ps`](#`docker-ps`)
    - [`docker search`](#`docker-search`)
    - [`docker stats`](#`docker-stats`)
    - [`docker system df`](#`docker-system-df`)
    - [`docker top`](#`docker-top`)
    - [`docker version`](#`docker-version`)
    - [`docker volume inspect`](#`docker-volume-inspect`)
    - [`docker volume ls`](#`docker-volume-ls`)
  - [Recipes](#recipes)

## Why?

Answering these with the `docker` CLI means wrestling with `--format` templates and `grep`:

- *which stopped containers exited with a non-zero code?*
- *how much disk can I reclaim from dangling images?*
- *which container is publishing to host port 5432?*

Because `dockerw` returns real Nushell values, you filter, sort, sum, and join them like any data:

```nu
# containers that died badly
docker ps -a --status exited | where exit_code != 0 | select name exit_code

# dangling images and the space they hold
docker images --dangling | get size | math sum

# which container owns host port 5432?
docker ps -o wide | where ($it.ports.host_port | any {|p| $p == 5432}) | get name

```

## How the shadowing works

Nushell resolves the **longest matching internal command name** first. `dockerw` exports
multi-word defs like `def "docker ps"` and `def "docker container inspect"`. When one matches, it
wins over the external `docker`. When nothing matches (`docker run`, `docker build`, …), Nushell
falls back to the external binary, with your configured autocompleter.

```nu
use dockerw *       # the * is MANDATORY — see below

docker ps           # dockerw: structured table of containers (shadowed)
docker images       # dockerw: structured table of images   (shadowed)
docker run -it …    # real docker: falls straight through   (not shadowed)
docker --version    # real docker: falls straight through
```

- **`use dockerw *` — the `*` is required.** Without it the commands land under a `dockerw` namespace
- **Non-shadowed commands keep native completion** from your configured completer.

## Installation

```nu
# clone into one of your NU_LIB_DIRS
let dest = [($env.NU_LIB_DIRS | first) dockerw] | path join
git clone git@github.com:lassoColombo/dockerw.git $dest

# load it — the * is mandatory so the `docker …` defs shadow the real subcommands
use dockerw *
docker ps
```

To make it permanent, put `use dockerw *` in your `config.nu`.

### Requirements

- **Nushell 0.114+**
- A reachable **Docker daemon**. The socket is resolved from `$env.DOCKER_HOST` when it is a
  `unix://` URL, otherwise `/var/run/docker.sock`. TCP/TLS daemons are out of scope.
- **Unix-socket platforms only — no native Windows (for now).** The module talks to the daemon
  exclusively over a Unix socket (`http get --unix-socket`), so it runs on **Linux and macOS**.

## Differences from docekr commands

Dockerw implements all the flags of the corresponding docker command and adds some nushell-specific conveniences.

### Output modes

Every shadowed command **except `docker top`** takes `--output` (`-o`) with three levels (a
dockerw-specific addition; `top` is skipped because `-o` collides with `ps`'s own option forwarded
through `top`'s ps-args):

| mode | what you get |
| --- | --- |
| `compact` | primitive columns only — no nested lists/records. **Default when listing.** |
| `wide` | richer; keeps nested columns (ports, mounts, labels, …). **Default for one object.** |
| `full` | the raw, unshaped Docker API response. |

```nu
docker ps                         # compact list (default)
docker ps -o wide                 # list, now with nested ports/labels/…
docker container inspect redis    # wide detail (default for one object)
docker container inspect redis -o compact
docker ps -o full                 # the raw /containers/json array
```

For flat commands (`stats`, `diff`, `history`, `search`, `system df`) `compact == wide`; for the
uncurated single records (`info`, `version`) `wide == full`.

### Filtering

Docker's `-f status=running -f label=…` **cannot** be expressed in Nushell (a flag can't repeat).
So `dockerw` exposes each filter as its **own discrete, completable flag** instead of one repeated `--filter`:

```nu
docker ps --status running --network db-net --ancestor nginx
docker ps --label com.docker.compose.project=mole,com.docker.compose.service=db # note: we are filtering on two labels
docker images --dangling --label app=web
docker network ls --scope local --driver bridge
docker volume ls --dangling --name data
docker search nginx --official --stars 100
```

## Commands

| Command                                                 | Signature          | Description                                                             |
| ------------------------------------------------------- | ------------------ | ----------------------------------------------------------------------- |
| [`docker container inspect`](#docker-container-inspect) | `nothing -> any`   | Inspect one or more containers in full detail.                          |
| [`docker diff`](#docker-diff)                           | `nothing -> any`   | Filesystem changes in a container relative to its image.                |
| [`docker history`](#docker-history)                     | `nothing -> any`   | Show the build history (layers) of an image.                            |
| [`docker image inspect`](#docker-image-inspect)         | `nothing -> any`   | Inspect one or more images in full detail.                              |
| [`docker images`](#docker-images)                       | `nothing -> any`   | List images, one row per repository:tag.                                |
| [`docker info`](#docker-info)                           | `nothing -> any`   | Daemon-wide information (the `GET /info` record).                       |
| [`docker inspect`](#docker-inspect)                     | `nothing -> any`   | Inspect any object, auto-detecting its type.                            |
| [`docker network inspect`](#docker-network-inspect)     | `nothing -> any`   | Inspect one or more networks in full detail.                            |
| [`docker network ls`](#docker-network-ls)               | `nothing -> any`   | List networks as structured rows.                                       |
| [`docker port`](#docker-port)                           | `nothing -> any`   | Host port mappings published by a container.                            |
| [`docker ps`](#docker-ps)                               | `nothing -> any`   | List containers as structured, typed rows.                              |
| [`docker search`](#docker-search)                       | `nothing -> any`   | Search Docker Hub for images.                                           |
| [`docker stats`](#docker-stats)                         | `nothing -> any`   | Per-container CPU / memory / PID usage — a one-shot snapshot.         |
| [`docker system df`](#docker-system-df)                 | `nothing -> any`   | Disk-usage summary across images, containers, volumes, and build cache. |
| [`docker top`](#docker-top)                             | `nothing -> table` | List the processes running inside a container.                          |
| [`docker version`](#docker-version)                     | `nothing -> any`   | Daemon (server-side) version details (the `GET /version` record).       |
| [`docker volume inspect`](#docker-volume-inspect)       | `nothing -> any`   | Inspect one or more volumes in full detail.                             |
| [`docker volume ls`](#docker-volume-ls)                 | `nothing -> any`   | List volumes as structured rows.                                        |

### `docker container inspect`

Inspect one or more containers in full detail.

Exact wrapper of `docker container inspect`. Returns the curated per-container  
detail record — config, state, mounts, connected networks, env, labels, and the  
port map as typed `{host_ip, host_port, container_port, proto}` records — or the  
raw API response with `-o full`. A single ref returns one record; multiple refs  
return a list of records.

**Signature:** `nothing -> any`

**Parameters**

| Parameter      | Type     | Description                        |
| -------------- | -------- | ---------------------------------- |
| `...container` | `string` | one or more container names or ids |

**Flags**

| Flag             | Type     | Description                                                      |
| ---------------- | -------- | ---------------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: wide (single-object default) \| compact \| full (raw API) |

**Search terms:** `inspect`, `container`, `detail`, `config`, `mounts`, `networks`, `env`

**Examples**

```nu
# inspect one container
docker container inspect redis

# just its mounts
docker container inspect redis | get mounts

# inspect several, raw API shape
docker container inspect redis nginx -o full
```

### `docker diff`

Filesystem changes in a container relative to its image.

Exact wrapper of `docker diff`. Each row is `{kind, path}` where `kind` is  
`added`, `modified`, or `deleted` (decoded from the API's numeric `Kind`).

**Signature:** `nothing -> any`

**Parameters**

| Parameter   | Type     | Description              |
| ----------- | -------- | ------------------------ |
| `container` | `string` | the container to inspect |

**Flags**

| Flag             | Type     | Description                                                 |
| ---------------- | -------- | ----------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: compact (default) \| wide \| full (raw {Path, Kind}) |

**Search terms:** `diff`, `changes`, `filesystem`, `modified`, `added`, `deleted`

**Examples**

```nu
# all changes
docker diff redis

# only added paths
docker diff redis | where kind == added | get path
```

### `docker history`

Show the build history (layers) of an image.

Exact wrapper of `docker history`. One row per layer, newest first: `created`  
is a `datetime`, `size` a `filesize`, `created_by` the build instruction  
(whitespace-collapsed unless `--no-trunc`).

**Signature:** `nothing -> any`

**Parameters**

| Parameter | Type     | Description          |
| --------- | -------- | -------------------- |
| `image`   | `string` | the image to inspect |

**Flags**

| Flag             | Type     | Description                                           |
| ---------------- | -------- | ----------------------------------------------------- |
| `--no-trunc`     | `switch` | don't truncate ids or the build instruction           |
| `--quiet`, `-q`  | `switch` | output only layer ids (a list<string>)                |
| `--output`, `-o` | `string` | shape: compact (default) \| wide \| full (raw layers) |

**Search terms:** `history`, `layers`, `build`, `image`, `size`

**Examples**

```nu
# layer history
docker history postgres:16

# total image size from layers
docker history nginx | get size | math sum
```

### `docker image inspect`

Inspect one or more images in full detail.

Exact wrapper of `docker image inspect`. Returns the curated per-image detail —  
repo tags/digests, os/architecture, config (cmd, entrypoint, env, exposed ports),  
and labels — or the raw API response with `-o full`. A single ref returns one  
record; multiple refs return a list.

**Signature:** `nothing -> any`

**Parameters**

| Parameter  | Type     | Description                    |
| ---------- | -------- | ------------------------------ |
| `...image` | `string` | one or more image names or ids |

**Flags**

| Flag             | Type     | Description                                                      |
| ---------------- | -------- | ---------------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: wide (single-object default) \| compact \| full (raw API) |

**Search terms:** `inspect`, `image`, `detail`, `config`, `layers`, `digest`

**Examples**

```nu
# inspect an image
docker image inspect postgres:16

# its exposed ports
docker image inspect nginx | get exposed_ports
```

### `docker images`

List images, one row per repository:tag.

Exact wrapper of `docker images`. `created` is a `datetime` and `size` a  
`filesize`, so you can sort and sum them directly. An image with several tags  
yields one row per tag; untagged layers show as `<none>:<none>`. An optional  
`REPOSITORY[:TAG]` positional narrows to matching references. Filter keys are  
discrete flags; use `--filter {…}` for anything without a dedicated flag.

**Signature:** `nothing -> any`

**Parameters**

| Parameter     | Type     | Description                            |
| ------------- | -------- | -------------------------------------- |
| `repository?` | `string` | optional REPOSITORY[:TAG] to narrow to |

**Flags**

| Flag             | Type     | Default | Description                                                              |
| ---------------- | -------- | ------- | ------------------------------------------------------------------------ |
| `--all`, `-a`    | `switch` |         | include intermediate layers (default: hidden)                            |
| `--digests`      | `switch` |         | add a `digest` column                                                    |
| `--dangling`     | `switch` |         | only untagged images (for dangling=false use --filter {dangling: false}) |
| `--before`       | `string` |         | only images created before this one                                      |
| `--since`        | `string` |         | only images created after this one                                       |
| `--label`        | `string` |         | label(s), comma-separated: app=web,tier=db                               |
| `--filter`, `-f` | `record` | `{}`    | escape hatch, e.g. {reference: "postgres:*"}                             |
| `--no-trunc`     | `switch` |         | don't truncate the image id                                              |
| `--quiet`, `-q`  | `switch` |         | output only ids (a list<string>, deduped)                                |
| `--output`, `-o` | `string` |         | shape: compact (list default) \| wide \| full (raw API)                  |

**Search terms:** `images`, `ls`, `list`, `repository`, `tag`, `size`, `dangling`

**Examples**

```nu
# all images
docker images

# images over 1 GB, largest first
docker images | where size > 1gb | sort-by size --reverse

# one repository
docker images postgres

# dangling (untagged) images and their size
docker images --dangling | select repository tag size
```

### `docker info`

Daemon-wide information (the `GET /info` record).

Exact wrapper of `docker info`. This record is uncurated, so `wide` and `full`  
are identical; `compact` drops the nested list/record fields, leaving the scalar  
summary. Handy for pulling a single field with `get`.

**Signature:** `nothing -> any`

**Flags**

| Flag             | Type     | Description                                                      |
| ---------------- | -------- | ---------------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: wide (default, == full) \| compact (scalars only) \| full |

**Search terms:** `info`, `system`, `daemon`, `engine`

**Examples**

```nu
# the full info record
docker info

# number of running containers
docker info | get ContainersRunning

# just the scalar fields
docker info -o compact
```

### `docker inspect`

Inspect any object, auto-detecting its type.

Generic wrapper of `docker inspect`. For each ref it tries container -> image ->  
network -> volume in turn and returns the same curated detail as the matching  
`docker <type> inspect`. Anything else docker can inspect (plugins, swarm  
objects, …) falls back to `^docker inspect` parsed into a structured record.  
A single ref returns one record; multiple refs return a list. Prefer the  
type-specific commands when you know the type — they also complete the ref.

**Signature:** `nothing -> any`

**Parameters**

| Parameter | Type     | Description                               |
| --------- | -------- | ----------------------------------------- |
| `...ref`  | `string` | one or more object names/ids, of any type |

**Flags**

| Flag             | Type     | Description                                                      |
| ---------------- | -------- | ---------------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: wide (single-object default) \| compact \| full (raw API) |

**Search terms:** `inspect`, `detail`, `object`, `generic`, `auto-detect`

**Examples**

```nu
# inspect whatever this ref is
docker inspect redis

# mix types in one call
docker inspect redis postgres:16 bridge

# raw API response
docker inspect redis -o full
```

### `docker network inspect`

Inspect one or more networks in full detail.

Exact wrapper of `docker network inspect`. Returns the curated per-network detail  
— driver, scope, subnets/gateways, attached containers, options, labels — or the  
raw API response with `-o full`. A single ref returns one record; multiple refs  
return a list.

**Signature:** `nothing -> any`

**Parameters**

| Parameter    | Type     | Description                      |
| ------------ | -------- | -------------------------------- |
| `...network` | `string` | one or more network names or ids |

**Flags**

| Flag             | Type     | Description                                                      |
| ---------------- | -------- | ---------------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: wide (single-object default) \| compact \| full (raw API) |

**Search terms:** `inspect`, `network`, `detail`, `subnet`, `gateway`, `containers`

**Examples**

```nu
# inspect a network
docker network inspect bridge

# its subnets
docker network inspect bridge | get subnets

# containers attached to a network
docker network inspect my-net | get containers
```

### `docker network ls`

List networks as structured rows.

Exact wrapper of `docker network ls`. Every filter key is its own tab-completing  
flag; anything without a dedicated flag goes through `--filter {…}`. Use  
`docker network inspect` for the full detail (subnets, connected containers, …).

**Signature:** `nothing -> any`

**Flags**

| Flag             | Type     | Default | Description                                                                       |
| ---------------- | -------- | ------- | --------------------------------------------------------------------------------- |
| `--type`         | `string` |         | network type: custom \| builtin                                                   |
| `--scope`        | `string` |         | scope: swarm \| global \| local                                                   |
| `--driver`       | `string` |         | driver: bridge\|host\|overlay\|macvlan\|ipvlan\|none                              |
| `--name`         | `string` |         | name (substring match)                                                            |
| `--id`           | `string` |         | network id                                                                        |
| `--dangling`     | `switch` |         | only networks unused by any container (for =false use --filter {dangling: false}) |
| `--label`        | `string` |         | label(s), comma-separated: app=web,tier=db                                        |
| `--filter`, `-f` | `record` | `{}`    | escape hatch for filter keys without a flag                                       |
| `--no-trunc`     | `switch` |         | don't truncate the network id                                                     |
| `--quiet`, `-q`  | `switch` |         | output only ids (a list<string>)                                                  |
| `--output`, `-o` | `string` |         | shape: compact (list default) \| wide \| full (raw API)                           |

**Search terms:** `network`, `ls`, `list`, `networks`, `driver`, `scope`, `bridge`, `overlay`

**Examples**

```nu
# all networks
docker network ls

# user-defined bridge networks
docker network ls --type custom --driver bridge

# networks unused by any container
docker network ls --dangling | get name
```

### `docker port`

Host port mappings published by a container.

Exact wrapper of `docker port`. Returns typed  
`{host_ip, host_port, container_port, proto}` records. With a `PRIVATE_PORT[/PROTO]`  
positional, only the mapping(s) for that container port (and protocol, if given)  
are returned.

**Signature:** `nothing -> any`

**Parameters**

| Parameter       | Type     | Description                                                       |
| --------------- | -------- | ----------------------------------------------------------------- |
| `container`     | `string` | the container to inspect                                          |
| `private_port?` | `string` | optional PRIVATE_PORT[/PROTO] to filter to, e.g. 6379 or 6379/tcp |

**Flags**

| Flag             | Type     | Description                                             |
| ---------------- | -------- | ------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: compact (default) \| wide \| full (raw port map) |

**Search terms:** `port`, `ports`, `published`, `mapping`, `expose`

**Examples**

```nu
# all published ports
docker port redis

# mapping for one container port
docker port redis 6379

# just the host ports
docker port nginx | get host_port
```

### `docker ps`

List containers as structured, typed rows.

Exact wrapper of `docker ps` — shadows the real subcommand and returns a  
queryable table instead of text: `created` is a `datetime`, `ports` a list of  
`{host_ip, host_port, container_port, proto}` records, and the human `status`  
string is decomposed into `state`, `health`, and `exit_code`. Bare it lists  
running containers; `--all` (or any all-states filter like `--exited`/`--last`)  
includes stopped ones. Every filter key is its own tab-completing flag; anything  
without a dedicated flag goes through the `--filter {…}` record escape hatch.

**Signature:** `nothing -> any`

**Flags**

| Flag             | Type     | Default | Description                                                             |
| ---------------- | -------- | ------- | ----------------------------------------------------------------------- |
| `--all`, `-a`    | `switch` |         | include stopped containers (default: running only)                      |
| `--status`       | `string` |         | run state: created\|restarting\|running\|removing\|paused\|exited\|dead |
| `--health`       | `string` |         | health: starting\|healthy\|unhealthy\|none                              |
| `--name`         | `string` |         | name (substring match)                                                  |
| `--ancestor`     | `string` |         | created from this image (name, id, or name:tag)                         |
| `--id`           | `string` |         | container id                                                            |
| `--network`      | `string` |         | connected to this network (name or id)                                  |
| `--volume`       | `string` |         | mounts this volume (name or mount path)                                 |
| `--exited`       | `int`    |         | exit code (implies --all)                                               |
| `--before`       | `string` |         | only containers created before this one                                 |
| `--since`        | `string` |         | only containers created after this one                                  |
| `--label`        | `string` |         | label(s), comma-separated: app=web,tier=db                              |
| `--filter`, `-f` | `record` | `{}`    | escape hatch for filter keys without a flag, e.g. {isolation: default}  |
| `--last`, `-n`   | `int`    |         | the n most recently created containers (any state)                      |
| `--latest`, `-l` | `switch` |         | only the most recently created container (any state)                    |
| `--no-trunc`     | `switch` |         | don't truncate the container id to 12 chars                             |
| `--quiet`, `-q`  | `switch` |         | output only ids (a list<string>)                                        |
| `--size`, `-s`   | `switch` |         | include each container's writable-layer size                            |
| `--output`, `-o` | `string` |         | shape: compact (list default) \| wide \| full (raw API)                 |

**Search terms:** `ps`, `ls`, `list`, `containers`, `running`, `state`, `ports`

**Examples**

```nu
# running containers (compact table)
docker ps

# include stopped containers
docker ps --all

# which container owns host port 5432?
docker ps -o wide | where ($it.ports | any {|p| $p.host_port == 5432}) | get name

# stopped containers that exited non-zero
docker ps -a | where state == exited and exit_code != 0 | select name exit_code

# filter by state and network (both complete live)
docker ps --status running --network bridge

# filter by compose labels
docker ps --label com.docker.compose.project=mole,com.docker.compose.service=db
```

### `docker search`

Search Docker Hub for images.

Exact wrapper of `docker search` — the one shadowed command that hits the  
network (Docker Hub) rather than the local daemon. Rows are  
`{name, stars, official, description}`, sorted by stars descending.

**Signature:** `nothing -> any`

**Parameters**

| Parameter | Type     | Description                       |
| --------- | -------- | --------------------------------- |
| `term`    | `string` | the term to search Docker Hub for |

**Flags**

| Flag             | Type     | Default | Description                                            |
| ---------------- | -------- | ------- | ------------------------------------------------------ |
| `--stars`        | `int`    |         | minimum star count                                     |
| `--official`     | `switch` |         | only official images (is-official=true)                |
| `--automated`    | `switch` |         | only automated builds (is-automated=true)              |
| `--filter`, `-f` | `record` | `{}`    | escape hatch for other search filter keys              |
| `--limit`        | `int`    |         | cap the number of results                              |
| `--no-trunc`     | `switch` |         | don't truncate the description column                  |
| `--output`, `-o` | `string` |         | shape: compact (default) \| wide \| full (raw results) |

**Search terms:** `search`, `hub`, `find`, `registry`, `stars`, `official`

**Examples**

```nu
# search Docker Hub
docker search nginx

# popular official images only
docker search postgres --official --stars 100 --limit 10
```

### `docker stats`

Per-container CPU / memory / PID usage — a one-shot snapshot.

Exact wrapper of `docker stats`, always one-shot (never a live stream). With no  
arguments it covers every running container; `--all` includes stopped ones. CPU%  
is computed from the single sample the daemon returns — it waits two cycles  
server-side so `precpu` is populated. Targets are queried concurrently. `mem`  
and `limit` are `filesize`; `cpu%`/`mem%` are rounded floats; `pids` an int.

**Signature:** `nothing -> any`

**Parameters**

| Parameter      | Type     | Description                                 |
| -------------- | -------- | ------------------------------------------- |
| `...container` | `string` | containers to sample (default: all running) |

**Flags**

| Flag             | Type     | Description                                          |
| ---------------- | -------- | ---------------------------------------------------- |
| `--all`, `-a`    | `switch` | sample stopped containers too (they report zeros)    |
| `--output`, `-o` | `string` | shape: compact (default) \| wide \| full (raw stats) |

**Search terms:** `stats`, `cpu`, `memory`, `ram`, `usage`, `load`, `pids`

**Examples**

```nu
# running containers, busiest CPU first
docker stats | sort-by "cpu%" --reverse

# specific containers
docker stats redis postgres

# memory hogs
docker stats | sort-by mem --reverse | select name mem "mem%"
```

### `docker system df`

Disk-usage summary across images, containers, volumes, and build cache.

Exact wrapper of `docker system df`. One row per resource type with `total`,  
`active`, and reclaimable `size` (a `filesize`) — the structured equivalent of  
the CLI's summary table.

**Signature:** `nothing -> any`

**Flags**

| Flag             | Type     | Description                                                 |
| ---------------- | -------- | ----------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: compact (default) \| wide \| full (raw usage record) |

**Search terms:** `system`, `df`, `disk`, `usage`, `space`, `reclaim`, `prune`

**Examples**

```nu
# disk-usage summary
docker system df

# total space used by all types
docker system df | get size | math sum
```

### `docker top`

List the processes running inside a container.

Exact wrapper of `docker top CONTAINER [ps OPTIONS]`. Cells are typed by column:  
PID/PPID/C/UID/RSS/… -> int, %CPU/%MEM -> float, TIME/TIME+ -> duration; anything  
else stays a string. Extra positional args are forwarded verbatim as the `ps`  
invocation inside the container. This is the one shadowed command with no  
`--output`: `-o` is reserved so it can be forwarded to `ps` as a ps-arg.

**Signature:** `nothing -> table`

**Parameters**

| Parameter    | Type     | Description                                                  |
| ------------ | -------- | ------------------------------------------------------------ |
| `container`  | `string` | the container to inspect                                     |
| `...ps_args` | `string` | optional `ps` arguments forwarded to the daemon (e.g. `aux`) |

**Search terms:** `top`, `ps`, `processes`, `threads`, `pid`

**Examples**

```nu
# processes in a container
docker top mole-psql-local-dev

# process count
docker top redis | length

# just pids and commands
docker top redis | select PID CMD
```

### `docker version`

Daemon (server-side) version details (the `GET /version` record).

Exact wrapper of `docker version` — server side only (no client section). The  
record is uncurated, so `wide` == `full`; `compact` keeps only scalar fields.

**Signature:** `nothing -> any`

**Flags**

| Flag             | Type     | Description                                                      |
| ---------------- | -------- | ---------------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: wide (default, == full) \| compact (scalars only) \| full |

**Search terms:** `version`, `api`, `build`, `server`, `engine`

**Examples**

```nu
# version details
docker version

# the negotiated API version
docker version | get ApiVersion
```

### `docker volume inspect`

Inspect one or more volumes in full detail.

Exact wrapper of `docker volume inspect`. Returns the curated per-volume detail —  
driver, scope, mountpoint, `created` (`datetime`), options, labels — or the raw  
API response with `-o full`. A single ref returns one record; multiple refs  
return a list.

**Signature:** `nothing -> any`

**Parameters**

| Parameter   | Type     | Description              |
| ----------- | -------- | ------------------------ |
| `...volume` | `string` | one or more volume names |

**Flags**

| Flag             | Type     | Description                                                      |
| ---------------- | -------- | ---------------------------------------------------------------- |
| `--output`, `-o` | `string` | shape: wide (single-object default) \| compact \| full (raw API) |

**Search terms:** `inspect`, `volume`, `detail`, `mountpoint`, `driver`, `options`

**Examples**

```nu
# inspect a volume
docker volume inspect devutils_dumbo-postgres-data

# just the mountpoint
docker volume inspect my-vol | get mountpoint
```

### `docker volume ls`

List volumes as structured rows.

Exact wrapper of `docker volume ls`. Filter keys are discrete tab-completing  
flags; anything without one goes through `--filter {…}`. Use  
`docker volume inspect` for the full detail (mountpoint, options, timestamps).

**Signature:** `nothing -> any`

**Flags**

| Flag             | Type     | Default | Description                                                                      |
| ---------------- | -------- | ------- | -------------------------------------------------------------------------------- |
| `--dangling`     | `switch` |         | only volumes unused by any container (for =false use --filter {dangling: false}) |
| `--driver`       | `string` |         | volume driver (e.g. local)                                                       |
| `--name`         | `string` |         | name (substring match)                                                           |
| `--label`        | `string` |         | label(s), comma-separated: app=web,tier=db                                       |
| `--filter`, `-f` | `record` | `{}`    | escape hatch for filter keys without a flag                                      |
| `--quiet`, `-q`  | `switch` |         | output only names (a list<string>)                                               |
| `--output`, `-o` | `string` |         | shape: compact (list default) \| wide \| full (raw API)                          |

**Search terms:** `volume`, `ls`, `list`, `volumes`, `driver`, `dangling`, `mountpoint`

**Examples**

```nu
# all volumes
docker volume ls

# volumes unused by any container
docker volume ls --dangling | get name

# local-driver volumes named like data
docker volume ls --driver local --name data
```

## Recipes

```nu
# containers ranked by memory use
docker stats | sort-by mem --reverse | select name mem "mem%"

# every published port mapping, as a flat table
docker ps -o wide
| each {|c| $c.ports | each {|p| {container: $c.name, host_port: $p.host_port, target: $p.container_port} } }
| flatten

# dangling images and the space they hold
docker images --dangling | select repository tag size

# largest images
docker images | sort-by size --reverse | first 5 | select repository tag size

# want docker's native implementation? simply call the binary
^docker ps --format '{{.Names}}: {{.Status}}'
```
