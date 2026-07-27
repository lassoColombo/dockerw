# nu-docker-shim

**Structured, typed, read-only Docker introspection for Nushell.**

> `nu-docker-shim` can override the default docker introspection commands with its own implementation.  
> You keep typing `docker ps`, but you get back Nushell values instead of text.  

> In alternative, it can be used as a standalone module alongside the docker command:  
> you type `docker-shim ps` or whatever short alias you like and you get structured data. 

---

- [nu-docker-shim](#nu-docker-shim)
  - [Why?](#why?)
  - [What nu-docker-shim is - and what it isn't](#what-nu-docker-shim-is---and-what-it-isn't)
  - [Installation](#installation)
    - [Requirements](#requirements)
  - [Configuration](#configuration)
    - [Configure as a module](#configure-as-a-module)
    - [Configure as a shim](#configure-as-a-shim)
  - [How nu-docker-shim works as of now](#how-nu-docker-shim-works-as-of-now)
    - [How the transport works](#how-the-transport-works)
    - [How the shadowing works](#how-the-shadowing-works)
  - [Differences from docker commands](#differences-from-docker-commands)
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

---

## Why?

Answering these with the `docker` CLI means wrestling with `--format` templates and `grep`:

- *which stopped containers exited with a non-zero code?*
- *how much disk can I reclaim from dangling images?*
- *which container is publishing to host port 5432?*

Because `nu-docker-shim` returns real Nushell values, you filter, sort, sum, and join them like any data:

```nu
# containers that died badly
docker ps -a --status exited | where exit_code != 0 | select name exit_code

# dangling images and the space they hold
docker images --dangling | get size | math sum

# which container owns host port 5432?
docker ps -o wide | where ($it.ports.host_port | any {|p| $p == 5432}) | get name

```

## What nu-docker-shim is - and what it isn't

**nu-docker-shim is:**

- A module that give you structured, typed output for most of the docker introspection commands 
- A shadower shim that expeses docker's flags in a more nushell idiomatic syntax.
- A way to get a shaped view of docker objects (`-o compact|wide`) **or** the raw API response from the server (`-o full`).

**It is not:**

- **It's not a docker replacement.** Only the introspection subset above is shadowed. `run`, `build`,
  `exec`, `logs`, `events`, `pull`/`push`, `compose`, … are **not** - they fall through to real docker.
- **It doesn't re-implement docker's transport or auth.** It delegates them to the `docker` CLI
  (via a `socat` → `docker system dial-stdio` bridge), so it reaches exactly the daemon `docker`
  does - local socket, remote `tcp://`+TLS, `ssh://`, Docker Desktop, rootless, per the active
  `docker context` / `$env.DOCKER_HOST`. It therefore needs the `docker` CLI (and `socat`) present.
- **No native Windows** - the bridge is driven from `bash`/`socat`, so it runs on Linux and macOS
  (incl. WSL2), not native Windows.

## Installation

```nu
# clone into one of your NU_LIB_DIRS
let dest = [($env.NU_LIB_DIRS | first) nu-docker-shim] | path join
git clone git@github.com:lassoColombo/nu-docker-shim.git $dest

# load it (see configuration section)
use nu-docker-shim
nu-docker-shim ps
```

### Requirements

- **Nushell 0.114+**
- The **`docker` CLI** and **`socat`** on `PATH`. Transport is delegated to docker (see
  [How the transport works](#how-the-transport-works)), so nu-docker-shim works against **any daemon
  `docker` can reach** - local socket, remote `tcp://`+TLS, `ssh://`, per the active `docker context`
  - with no socket-path or `DOCKER_HOST` rules of its own. Install socat if missing
  (`brew install socat`, `apt install socat`, …).
- **Linux and macOS (incl. WSL2), no native Windows** - the bridge relies on `bash`/`socat`.

## Configuration 

Nu-docker-shim can be configured either as a shim or as a standalone module.  

When configured as a shim, it will shadow some of the docker commands:  
You type `docker ps`, but you get back Nushell values instead of text.  

When configured as a standalone module it can be used alongside the docker command:  
`docker ps` will return text, but `docker-shim ps` will return structured data.

Both flavours are the same implementation - just imported differently.

### Configure as a module

The default. Commands live under the module name, so they sit **alongside** the real
`docker` command without shadowing it. Add to your `config.nu`:

```nu
use nu-docker-shim

nu-docker-shim ps                       # structured containers
nu-docker-shim volume ls                # structured volumes
docker ps                               # still the real docker (text)
```

The `nu-docker-shim` prefix is long. Nushell has no `use … as` to rename a module prefix,
so wrap the import in a module of whatever name you like:

```nu
module ds { export use nu-docker-shim * }
use ds

ds ps
ds container inspect redis
```

> Avoid bare `use nu-docker-shim *` (unprefixed): it would drop `ps`, `top`, `inspect`, … into
> your scope, shadowing Nushell's built-ins of those names. Use a prefix (module or wrapper).

### Configure as a shim

Shadows the matching `docker …` subcommands: you type `docker ps`, you get Nushell values.
Load the nested `shim` submodule **with the `*`**. Add to your `config.nu`:

```nu
use nu-docker-shim shim *

docker ps           # nu-docker-shim: structured table of containers (shadowed)
docker images       # nu-docker-shim: structured table of images   (shadowed)
docker run -it …    # real docker: falls straight through   (not shadowed)
docker --version    # real docker: falls straight through
```

- **The `*` is required.** Without it the commands land under a `nu-docker-shim shim` namespace - useless.
- **Non-shadowed commands keep native completion** from your configured completer.

## How nu-docker-shim works as of now

There is **one implementation** - facade-neutral commands like `ps`, `container inspect`,
`network ls` - behind the two facades you choose at import (module vs shim, see
[Configuration](#configuration)). Each structured command is a thin wrapper that does three things:

1. **Builds a Docker Engine API request** - the same REST call the `docker` CLI makes internally
   (`docker ps` → `GET /v1.47/containers/json`), including its query parameters and filters.
2. **Sends it through the transport** and gets back the raw JSON response from the daemon.
3. **Shapes that JSON** into typed, queryable columns - the `-o compact|wide|full` levels
   (see [Output modes](#output-modes)).

```
docker ps
  → ps                        builds  GET /v1.47/containers/json?all=…&filters=…
  → transport                 http get --unix-socket <bridge>  http://localhost/v1.47/containers/json?…
      → socat  (UNIX-LISTEN, fork)
          → docker system dial-stdio     ← docker picks & connects the daemon
              → Docker Engine API            (context / DOCKER_HOST / TLS / ssh)
  ← JSON  →  shaped into a typed table  (compact | wide | full)
```

### How the transport works

nu-docker-shim does **not** open the daemon connection itself. It has no code for unix sockets,
TCP+TLS, `ssh://` tunnels, client certificates, or `docker context` resolution - and that is
deliberate: this is exactly the transport and authentication logic the `docker` CLI already
implements. Re-implementing it would be a large, fragile duplication. So instead nu-docker-shim
**borrows docker's own connection**, in three layers:

- **`docker system dial-stdio`** - a hidden docker subcommand that opens a raw byte stream to
  *whatever daemon docker would talk to* and pipes it to stdin/stdout. docker does all the work of
  selecting and connecting the daemon (active `docker context`, `$env.DOCKER_HOST`, TLS certs,
  `ssh://` tunnels).
- **`socat`** exposes that stream as an ordinary **local unix socket**:
  `socat UNIX-LISTEN:<sock>,fork EXEC:'docker system dial-stdio'`. Every connection to `<sock>` forks
  a fresh `dial-stdio` - i.e. a fresh daemon connection.
- **Nushell's native `http get --unix-socket <sock>`** speaks the Engine API over that socket.
  Nushell handles all the HTTP framing (chunked bodies, content-length, JSON decoding); nothing is
  parsed by hand.

The net effect: **docker owns transport + auth, Nushell owns HTTP, and nu-docker-shim owns neither** -
so it reaches any daemon `docker` can, local or remote, without a line of transport code of its own.

**Bridge lifecycle.** The bridge is started lazily on the first request and reused after that:

- It is **keyed by target** (`$env.DOCKER_HOST` + `$env.DOCKER_CONTEXT`), so pointing at a different
  daemon via env spins up a separate bridge. Switching with `docker context use` is picked up
  automatically - each connection re-execs `dial-stdio`, which re-reads `~/.docker/config.json`.
- One `socat` per target is shared across commands **and across shells** (like an `ssh`
  ControlMaster), so the per-request cost is just a native `http get` (~0.1 s). The bridge
  **lingers until killed or reboot** - there is no idle self-shutdown.
- Startup is race-free: concurrent first-uses (e.g. `docker stats`, which fans out over containers)
  coordinate via a lock, and readiness is confirmed by polling `/_ping` before the first real
  request. A dead or wedged bridge is detected and rebuilt once.
- The socket lives in `$XDG_RUNTIME_DIR` (falling back to `$TMPDIR`) at mode `0600`; it proxies
  straight to the daemon, so it is deliberately not world-connectable.

This layering is also why the module needs the `docker` CLI **and** `socat` on `PATH`, and runs on
Linux/macOS/WSL2 but not native Windows (see [Requirements](#requirements)).

### How the shadowing works

Nushell resolves the **longest matching internal command name** first. The shim exports
aliases like `docker ps` and `docker container inspect` onto the structured commands.
When one matches, it wins over the external `docker`. When nothing matches (`docker run`,
`docker build`, …), Nushell falls back to the external binary, with your configured
autocompleter. The aliases inherit each command's full signature and completers, so
`docker ps --status <TAB>` completes natively.

## Differences from docker commands

nu-docker-shim implements all the flags of the corresponding docker command and adds some nushell-specific conveniences.

### Output modes

Every shadowed command **except `docker top`** takes `--output` (`-o`) with three levels (a
nu-docker-shim-specific addition; `top` is skipped because `-o` collides with `ps`'s own option forwarded
through `top`'s ps-args):

| mode | what you get |
| --- | --- |
| `compact` | primitive columns only - no nested lists/records. **Default when listing.** |
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
So `nu-docker-shim` exposes each filter as its **own discrete, completable flag** instead of one repeated `--filter`:

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
| [`docker stats`](#docker-stats)                         | `nothing -> any`   | Per-container CPU / memory / PID usage - a one-shot snapshot.         |
| [`docker system df`](#docker-system-df)                 | `nothing -> any`   | Disk-usage summary across images, containers, volumes, and build cache. |
| [`docker top`](#docker-top)                             | `nothing -> table` | List the processes running inside a container.                          |
| [`docker version`](#docker-version)                     | `nothing -> any`   | Daemon (server-side) version details (the `GET /version` record).       |
| [`docker volume inspect`](#docker-volume-inspect)       | `nothing -> any`   | Inspect one or more volumes in full detail.                             |
| [`docker volume ls`](#docker-volume-ls)                 | `nothing -> any`   | List volumes as structured rows.                                        |

### `docker container inspect`

Inspect one or more containers in full detail.

Exact wrapper of `docker container inspect`. Returns the curated per-container  
detail record - config, state, mounts, connected networks, env, labels, and the  
port map as typed `{host_ip, host_port, container_port, proto}` records - or the  
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

Exact wrapper of `docker image inspect`. Returns the curated per-image detail -  
repo tags/digests, os/architecture, config (cmd, entrypoint, env, exposed ports),  
and labels - or the raw API response with `-o full`. A single ref returns one  
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
type-specific commands when you know the type - they also complete the ref.

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
- driver, scope, subnets/gateways, attached containers, options, labels - or the  
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

Exact wrapper of `docker ps` - shadows the real subcommand and returns a  
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

Exact wrapper of `docker search` - the one shadowed command that hits the  
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

Per-container CPU / memory / PID usage - a one-shot snapshot.

Exact wrapper of `docker stats`, always one-shot (never a live stream). With no  
arguments it covers every running container; `--all` includes stopped ones. CPU%  
is computed from the single sample the daemon returns - it waits two cycles  
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
`active`, and reclaimable `size` (a `filesize`) - the structured equivalent of  
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

Exact wrapper of `docker version` - server side only (no client section). The  
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

Exact wrapper of `docker volume inspect`. Returns the curated per-volume detail -  
driver, scope, mountpoint, `created` (`datetime`), options, labels - or the raw  
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
