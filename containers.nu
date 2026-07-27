# Container commands — exact wrappers of `docker ps`, `docker container inspect`,
# `docker stats`, `docker top`, `docker diff`. Same flags as docker; the output
# is structured/typed instead of text. `--output (-o) compact|wide|full` is a
# nu-docker-shim-specific addition on `ps` / `container inspect` (docker has no such
# flag and `-o` is free on these commands).
#
# Commands are named facade-neutrally (`ps`, `container inspect`, …); the docker-
# shadowing names (`docker ps`, …) are added by shim.nu. Assembled by core.nu.
# See mod.nu's header for the two facades.
use client.nu
use common.nu

# --- completions ---
# Container refs are completed by `common complete-container` (shared). The
# enum-valued `docker ps` filters get their own static completers:
def complete-status []: nothing -> list<string> { [created restarting running removing paused exited dead] }
def complete-health []: nothing -> list<string> { [starting healthy unhealthy none] }

# Live label completion (comma-separated), sourced from real container labels.
def complete-container-label [context: string] {
  common complete-label $context (common harvest-labels (try {
    client containers-json list --unix-socket (common docker-socket) --all true
  } catch { [] }))
}

# --- shapers ---

# Shape one row of the container LIST endpoint (the "wide" list view).
def shape-container-summary [c: record, full_id: bool]: nothing -> record {
  let st = ($c.Status | common parse-status)
  {
    id: (if $full_id { $c.Id } else { $c.Id | common short-id })
    name: ($c.Names | get 0 | str replace -r '^/' '')
    image: $c.Image
    state: $c.State
    health: $st.health
    exit_code: $st.exit_code
    created: ($c.Created | common epoch-to-datetime)
    command: $c.Command
    ports: ($c.Ports | common parse-ports)
    labels: $c.Labels
  }
}

# Shape a container INSPECT response (the "wide" detail view).
def shape-container-detail [c: record]: nothing -> record {
  {
    id: ($c.Id | common short-id)
    name: ($c.Name | str replace -r '^/' '')
    image: $c.Config.Image
    state: $c.State.Status
    health: ($c.State.Health?.Status?)
    exit_code: $c.State.ExitCode
    created: ($c.Created | common iso-to-datetime)
    started_at: ($c.State.StartedAt | common iso-to-datetime)
    restart_count: $c.RestartCount
    command: ([$c.Path] | append $c.Args | str join ' ')
    ports: ($c.NetworkSettings.Ports | common parse-ports-map)
    mounts: ($c.Mounts | each {|m| {type: $m.Type?, source: $m.Source?, destination: $m.Destination?, rw: $m.RW?}})
    networks: ($c.NetworkSettings.Networks | columns)
    env: ($c.Config.Env?)
    labels: ($c.Config.Labels?)
  }
}

# --- commands ---

# List containers as structured, typed rows.
#
# Exact wrapper of `docker ps` — shadows the real subcommand and returns a
# queryable table instead of text: `created` is a `datetime`, `ports` a list of
# `{host_ip, host_port, container_port, proto}` records, and the human `status`
# string is decomposed into `state`, `health`, and `exit_code`. Bare it lists
# running containers; `--all` (or any all-states filter like `--exited`/`--last`)
# includes stopped ones. Every filter key is its own tab-completing flag; anything
# without a dedicated flag goes through the `--filter {…}` record escape hatch.
@search-terms ps ls list containers running state ports
@example "running containers (compact table)" { docker ps }
@example "include stopped containers" { docker ps --all }
@example "which container owns host port 5432?" { docker ps -o wide | where ($it.ports | any {|p| $p.host_port == 5432}) | get name }
@example "stopped containers that exited non-zero" { docker ps -a | where state == exited and exit_code != 0 | select name exit_code }
@example "filter by state and network (both complete live)" { docker ps --status running --network bridge }
@example "filter by compose labels" { docker ps --label com.docker.compose.project=mole,com.docker.compose.service=db }
export def "ps" [
  --all (-a)                                        # include stopped containers (default: running only)
  --status: string@complete-status                  # run state: created|restarting|running|removing|paused|exited|dead
  --health: string@complete-health                  # health: starting|healthy|unhealthy|none
  --name: string@"common complete-container"        # name (substring match)
  --ancestor: string@"common complete-image"        # created from this image (name, id, or name:tag)
  --id: string@"common complete-container"          # container id
  --network: string@"common complete-network"       # connected to this network (name or id)
  --volume: string@"common complete-volume"         # mounts this volume (name or mount path)
  --exited: int                                     # exit code (implies --all)
  --before: string@"common complete-container"      # only containers created before this one
  --since: string@"common complete-container"       # only containers created after this one
  --label: string@complete-container-label          # label(s), comma-separated: app=web,tier=db
  --filter (-f): record = {}                        # escape hatch for filter keys without a flag, e.g. {isolation: default}
  --last (-n): int                                  # the n most recently created containers (any state)
  --latest (-l)                                     # only the most recently created container (any state)
  --no-trunc                                        # don't truncate the container id to 12 chars
  --quiet (-q)                                       # output only ids (a list<string>)
  --size (-s)                                        # include each container's writable-layer size
  --output (-o): string@"common output-completer"   # shape: compact (list default) | wide | full (raw API)
]: nothing -> any {
  let sock = (common docker-socket)
  let all2 = ($all or $latest or ($last != null) or ($exited != null))
  let filters = ($filter | common add-filters {
    status: $status, health: $health, name: $name, ancestor: $ancestor, id: $id,
    network: $network, volume: $volume, exited: $exited, before: $before, since: $since
    label: (if ($label | is-empty) { null } else { $label | split row ',' })
  })
  let raw0 = (client containers-json list --unix-socket $sock --all $all2 --size $size --filters (common build-filters $filters))
  let raw = if $latest { $raw0 | first 1 } else if ($last != null) { $raw0 | first $last } else { $raw0 }
  if $quiet {
    return ($raw | each {|c| if $no_trunc { $c.Id } else { $c.Id | common short-id }})
  }
  let wide = ($raw | each {|c|
    let base = (shape-container-summary $c $no_trunc)
    if $size { $base | insert size (($c.SizeRw? | default 0) | into filesize) } else { $base }
  })
  common render-output $output $raw $wide --list
}

# Inspect one or more containers in full detail.
#
# Exact wrapper of `docker container inspect`. Returns the curated per-container
# detail record — config, state, mounts, connected networks, env, labels, and the
# port map as typed `{host_ip, host_port, container_port, proto}` records — or the
# raw API response with `-o full`. A single ref returns one record; multiple refs
# return a list of records.
@search-terms inspect container detail config mounts networks env
@example "inspect one container" { docker container inspect redis }
@example "just its mounts" { docker container inspect redis | get mounts }
@example "inspect several, raw API shape" { docker container inspect redis nginx -o full }
export def "container inspect" [
  ...container: string@"common complete-container"   # one or more container names or ids
  --output (-o): string@"common output-completer"    # shape: wide (single-object default) | compact | full (raw API)
]: nothing -> any {
  if ($container | is-empty) { error make {msg: '"docker container inspect" requires at least 1 argument'} }
  let sock = (common docker-socket)
  let raws = ($container | each {|c| client containers-json get $c --unix-socket $sock })
  let wides = ($raws | each {|r| shape-container-detail $r })
  if (($container | length) == 1) {
    common render-output $output ($raws | first) ($wides | first)
  } else {
    common render-output $output $raws $wides
  }
}

# Per-container CPU / memory / PID usage — a one-shot snapshot.
#
# Exact wrapper of `docker stats`, always one-shot (never a live stream). With no
# arguments it covers every running container; `--all` includes stopped ones. CPU%
# is computed from the single sample the daemon returns — it waits two cycles
# server-side so `precpu` is populated. Targets are queried concurrently. `mem`
# and `limit` are `filesize`; `cpu%`/`mem%` are rounded floats; `pids` an int.
@search-terms stats cpu memory ram usage load pids
@example "running containers, busiest CPU first" { docker stats | sort-by "cpu%" --reverse }
@example "specific containers" { docker stats redis postgres }
@example "memory hogs" { docker stats | sort-by mem --reverse | select name mem "mem%" }
export def "stats" [
  ...container: string@"common complete-container"   # containers to sample (default: all running)
  --all (-a)                                         # sample stopped containers too (they report zeros)
  --output (-o): string@"common output-completer"    # shape: compact (default) | wide | full (raw stats)
]: nothing -> any {
  let sock = (common docker-socket)
  let targets = if ($container | is-not-empty) { $container } else {
    client containers-json list --unix-socket $sock --all $all | each {|c| $c.Names | get 0 | str replace -r '^/' '' }
  }
  let raw = ($targets | par-each {|c| client containers-stats stats $c --unix-socket $sock --stream false } | sort-by name)
  let wide = ($raw | each {|s|
    let cpu_delta = (($s.cpu_stats.cpu_usage.total_usage? | default 0) - ($s.precpu_stats.cpu_usage.total_usage? | default 0))
    let sys_delta = (($s.cpu_stats.system_cpu_usage? | default 0) - ($s.precpu_stats.system_cpu_usage? | default 0))
    let ncpu = ($s.cpu_stats.online_cpus? | default 1)
    let cpu = if $sys_delta > 0 { ($cpu_delta / $sys_delta) * $ncpu * 100 } else { 0.0 }
    let used = (($s.memory_stats.usage? | default 0) - ($s.memory_stats.stats?.inactive_file? | default 0))
    let limit = ($s.memory_stats.limit? | default 0)
    {
      name: ($s.name | str replace -r '^/' '')
      "cpu%": ($cpu | math round --precision 2)
      mem: ($used | into filesize)
      limit: ($limit | into filesize)
      "mem%": (if $limit > 0 { ($used / $limit * 100) | math round --precision 2 } else { 0.0 })
      pids: ($s.pids_stats.current? | default 0)
    }
  })
  common render-output $output $raw $wide --list
}

# List the processes running inside a container.
#
# Exact wrapper of `docker top CONTAINER [ps OPTIONS]`. Cells are typed by column:
# PID/PPID/C/UID/RSS/… -> int, %CPU/%MEM -> float, TIME/TIME+ -> duration; anything
# else stays a string. Extra positional args are forwarded verbatim as the `ps`
# invocation inside the container. This is the one shadowed command with no
# `--output`: `-o` is reserved so it can be forwarded to `ps` as a ps-arg.
@search-terms top ps processes threads pid
@example "processes in a container" { docker top mole-psql-local-dev }
@example "process count" { docker top redis | length }
@example "just pids and commands" { docker top redis | select PID CMD }
export def "top" [
  container: string@"common complete-container"   # the container to inspect
  ...ps_args: string                              # optional `ps` arguments forwarded to the daemon (e.g. `aux`)
]: nothing -> table {
  let sock = (common docker-socket)
  let r = if ($ps_args | is-empty) {
    client containers-top top $container --unix-socket $sock
  } else {
    client containers-top top $container --unix-socket $sock --ps-args ($ps_args | str join ' ')
  }
  let titles = $r.Titles
  $r.Processes | each {|proc|
    let typed = ($proc | enumerate | each {|it| common coerce-top-cell ($titles | get $it.index) $it.item })
    $titles | zip $typed | into record
  }
}

# Filesystem changes in a container relative to its image.
#
# Exact wrapper of `docker diff`. Each row is `{kind, path}` where `kind` is
# `added`, `modified`, or `deleted` (decoded from the API's numeric `Kind`).
@search-terms diff changes filesystem modified added deleted
@example "all changes" { docker diff redis }
@example "only added paths" { docker diff redis | where kind == added | get path }
export def "diff" [
  container: string@"common complete-container"   # the container to inspect
  --output (-o): string@"common output-completer"   # shape: compact (default) | wide | full (raw {Path, Kind})
]: nothing -> any {
  let kinds = {"0": modified, "1": added, "2": deleted}
  let raw = (client containers-changes changes $container --unix-socket (common docker-socket) | default [])
  let wide = ($raw | each {|c| {kind: ($kinds | get ($c.Kind | into string)), path: $c.Path} })
  common render-output $output $raw $wide --list
}

# Host port mappings published by a container.
#
# Exact wrapper of `docker port`. Returns typed
# `{host_ip, host_port, container_port, proto}` records. With a `PRIVATE_PORT[/PROTO]`
# positional, only the mapping(s) for that container port (and protocol, if given)
# are returned.
@search-terms port ports published mapping expose
@example "all published ports" { docker port redis }
@example "mapping for one container port" { docker port redis 6379 }
@example "just the host ports" { docker port nginx | get host_port }
export def "port" [
  container: string@"common complete-container"   # the container to inspect
  private_port?: string                           # optional PRIVATE_PORT[/PROTO] to filter to, e.g. 6379 or 6379/tcp
  --output (-o): string@"common output-completer"   # shape: compact (default) | wide | full (raw port map)
]: nothing -> any {
  let raw = ((client containers-json get $container --unix-socket (common docker-socket)).NetworkSettings.Ports | default {})
  let all = ($raw | common parse-ports-map)
  let wide = if ($private_port | is-not-empty) {
    let parts = ($private_port | split row '/')
    let p = ($parts.0 | into int)
    let proto = ($parts | get 1? | default null)
    $all | where container_port == $p | where {|x| ($proto == null) or ($x.proto == $proto)}
  } else { $all }
  common render-output $output $raw $wide --list
}

# --- aliases: docker's object-subcommand forms of the commands above ---
export alias "container ls" = ps
export alias "container list" = ps
export alias "container ps" = ps
export alias "container stats" = stats
export alias "container top" = top
export alias "container diff" = diff
export alias "container port" = port
