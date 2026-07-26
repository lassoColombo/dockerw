# Shared internal helpers for the `nu-docker-shim` Docker-introspection module.
#
# Nothing here is part of the public `nu-docker-shim` surface — the wrapper files `use`
# these helpers to resolve the socket, encode filters, demultiplex log streams,
# format raw API values into typed, queryable columns, and complete object refs.

use client.nu

# Resolve the local Docker Engine API unix socket.
#
# nu-docker-shim talks to a local, unauthenticated daemon only (no TCP/TLS/ssh) —
# by design. Resolution order:
#   1. `$env.DOCKER_HOST`, if it is a `unix://` URL — its path is used verbatim.
#      A non-`unix://` DOCKER_HOST (e.g. `tcp://`, `ssh://`) is a hard error, not a
#      silent fall-through to the local socket: it means the user is pointing docker
#      at a daemon we can't reach, so we say so and defer to `^docker`.
#   2. otherwise the first of these that exists — covering standard Linux, Docker
#      Desktop (macOS/Linux, where `/var/run/docker.sock` is a symlink to it), and
#      rootless Linux (`$XDG_RUNTIME_DIR/docker.sock`).
# Errors clearly when none exist (daemon stopped / non-standard path).
export def docker-socket []: nothing -> string {
  let h = ($env | get -o DOCKER_HOST | default "")
  if ($h | str starts-with "unix://") { return ($h | str replace "unix://" "") }
  if ($h | is-not-empty) {
    error make --unspanned { msg: $"nu-docker-shim is local-socket only, but $env.DOCKER_HOST=($h) is not a unix:// URL. Unset it, or use `^docker` to reach that daemon." }
  }
  let xdg = ($env | get -o XDG_RUNTIME_DIR | default "")
  let candidates = ([
    "/var/run/docker.sock"                                # standard Linux / Docker Desktop symlink
    ($env.HOME | path join ".docker" "run" "docker.sock") # Docker Desktop real socket (macOS & Linux)
  ] | append (if ($xdg | is-not-empty) { [($xdg | path join "docker.sock")] } else { [] }))  # rootless
  let found = ($candidates | where {|p| $p | path exists })
  if ($found | is-empty) {
    error make --unspanned { msg: $"no local Docker socket found \(looked in: ($candidates | str join ', ')). Is Docker running?" }
  }
  $found | first
}

# Encode Docker's `filters` query value from a record whose values are each a
# string or a list of strings (a list stands in for docker's repeated key). This
# is the exact shape of docker's `map[string][]string`. Returns null when empty
# so callers omit the parameter.
#
#   {status: running, label: [app=web tier=db]}
#     -> {"status":["running"],"label":["app=web","tier=db"]}
export def build-filters [f: record]: nothing -> any {
  if ($f | is-empty) { return null }
  $f
  | items {|k v|
      let vals = if (($v | describe) | str starts-with "list") { $v } else { [$v] }
      {$k: ($vals | each {|x| $x | into string })}
    }
  | reduce --fold {} {|it acc| $acc | merge $it }
  | to json --raw
}

# Merge the discrete filter flags a command exposes (e.g. `--status`, `--scope`)
# into its `--filter` record, dropping any that are unset (null). A discrete
# flag wins over the same key in the base record.
#
#   {status: exited} | add-filters {status: "running", name: null} -> {status: "running"}
export def add-filters [extra: record]: record -> record {
  let base = $in
  $extra | transpose k v | where v != null | reduce --fold $base {|it acc| $acc | upsert $it.k $it.v }
}

# Complete a comma-separated `--label` filter string. `context` is the command
# line up to the cursor; `pairs` is the real `key=value` labels harvested from
# the daemon (each resource file passes its own). Only the segment after the last
# comma is being typed, so we preserve the earlier segments as a prefix (a
# completer replaces the whole token) and offer both bare `key=` prefixes and
# full `key=value` pairs.
export def complete-label [context: string, pairs: list<string>]: nothing -> record {
  let token = ($context | split row ' ' | last)
  let i = ($token | str index-of --end ',')
  let prefix = if $i >= 0 { $token | str substring 0..$i } else { "" }   # includes the comma
  let keys = ($pairs | each {|p| ($p | split row -n 2 '=' | get 0) + '=' })
  {
    options: {sort: false}
    completions: (($keys | append $pairs | uniq) | each {|c| {value: $"($prefix)($c)"} })
  }
}

# Harvest unique `key=value` label strings from a list of API records (each with
# an optional `Labels` map). Used by the per-resource `--label` completers.
export def harvest-labels [rows: list<any>]: nothing -> list<string> {
  $rows | each {|r| $r.Labels? | default {} | items {|k v| $"($k)=($v)"} } | flatten | uniq
}

# ---- shared object-ref completers ------------------------------------------
# Live completions sourced from the daemon, shared across resource files (e.g.
# `docker ps --network` completes real networks). Each is resilient: a socket
# error yields no candidates rather than breaking completion.

# Container refs by name (running and stopped), described `state · image` and
# colored by run state.
export def complete-container []: nothing -> record {
  {
    options: {sort: false}
    completions: (try {
      client containers-json list --unix-socket (docker-socket) --all true
      | each {|c| {
          value: ($c.Names | get 0 | str replace -r '^/' '')
          description: $"($c.State) · ($c.Image)"
          style: (match $c.State {
            "running" => "green"
            "paused" => "yellow"
            "restarting" | "created" => "cyan"
            "exited" | "dead" | "removing" => "red"
            _ => "white"
          })
        } }
    } catch { [] })
  }
}

# Image refs from local repo:tags (skipping untagged), described `size · short-id`.
export def complete-image []: nothing -> record {
  {
    options: {sort: false}
    completions: (try {
      client images-json list --unix-socket (docker-socket)
      | each {|img|
          let tags = ($img.RepoTags? | default [] | where $it != "<none>:<none>")
          $tags | each {|t| {
            value: $t
            description: $"(($img.Size? | default 0) | into filesize) · ($img.Id | short-id)"
          } }
        }
      | flatten
    } catch { [] })
  }
}

# Network refs by name, described `driver · scope`.
export def complete-network []: nothing -> record {
  {
    options: {sort: false}
    completions: (try {
      client networks list --unix-socket (docker-socket)
      | each {|n| {value: $n.Name, description: $"($n.Driver) · ($n.Scope)"} }
    } catch { [] })
  }
}

# Volume refs by name, described `driver · mountpoint`.
export def complete-volume []: nothing -> record {
  {
    options: {sort: false}
    completions: (try {
      client volumes list --unix-socket (docker-socket)
      | get -o Volumes | default []
      | each {|v| {value: $v.Name, description: $"($v.Driver) · ($v.Mountpoint)"} }
    } catch { [] })
  }
}

# Truncate a Docker object ID to its 12-char short form, dropping any `sha256:`
# prefix. Empty/null passes through unchanged.
export def short-id []: any -> any {
  let id = $in
  if ($id | is-empty) { return $id }
  $id | str replace "sha256:" "" | str substring 0..<12
}

# Convert a Unix timestamp in seconds (as the Docker API returns) into a real
# `datetime`. Null -> null. (`into datetime` reads a bare int as nanoseconds, so
# we add seconds to the epoch explicitly.)
export def epoch-to-datetime []: any -> any {
  let s = $in
  if $s == null { return null }
  (1970-01-01T00:00:00+00:00) + ($s * 1sec)
}

# Reshape a container's raw `Ports` (list of {IP?, PrivatePort, PublicPort?, Type})
# into a typed, queryable list of records: `host_ip` (string|null), `host_port`
# (int|null — null when the port is exposed but not published), `container_port`
# (int), `proto` (string). Ports are already ints on the wire, so this is a
# rename/restructure rather than the lossy string `docker ps` prints.
export def parse-ports []: any -> list {
  ($in | default []) | each {|p| {
    host_ip: ($p.IP?)
    host_port: ($p.PublicPort?)
    container_port: ($p.PrivatePort?)
    proto: ($p.Type? | default "tcp")
  } }
}

# Decompose a container's human `Status` string (e.g. "Up 7 days (healthy)",
# "Exited (137) 2 hours ago", "Restarting (1) …") into typed fields: `health`
# ("healthy"|"unhealthy"|"starting"|null) and `exit_code` (int|null). The
# discrete run state comes from the separate `State` field, so it is not
# re-derived here. Returns nulls for whatever the string does not carry.
export def parse-status []: any -> record {
  let s = ($in | default "")
  let hm = ($s | parse --regex '\((?<h>healthy|unhealthy|health: starting)\)')
  let health = if ($hm | is-empty) { null } else {
    let h = $hm.0.h
    if $h == "health: starting" { "starting" } else { $h }
  }
  let cm = ($s | parse --regex '\((?<c>\d+)\)')
  let exit_code = if ($cm | is-empty) { null } else { $cm.0.c | into int }
  {health: $health, exit_code: $exit_code}
}

# Parse a `ps`-style elapsed-time field into a `duration`. Handles `SS`,
# `MM:SS`, `HH:MM:SS`, and `D-HH:MM:SS`. e.g. "00:00:14" -> 14sec.
export def parse-ps-time [s: string]: nothing -> duration {
  let parts = ($s | split row "-")
  let days = if ($parts | length) > 1 { $parts.0 | into int } else { 0 }
  let hms = ($parts | last | split row ":" | reverse)
  let sec = ($hms | get 0? | default "0" | into int)
  let min = ($hms | get 1? | default "0" | into int)
  let hr  = ($hms | get 2? | default "0" | into int)
  ($days * 1day) + ($hr * 1hr) + ($min * 1min) + ($sec * 1sec)
}

# Coerce one `docker top` cell to a sensible type based on its column title.
# `ps` returns every value as a string; numeric process columns become `int`,
# CPU/mem percentages `float`, and the cumulative CPU `TIME` a `duration`.
# Unknown or non-numeric values (e.g. UID "root") pass through as strings.
export def coerce-top-cell [title: string, value: string]: nothing -> any {
  match $title {
    "PID" | "PPID" | "C" | "RSS" | "VSZ" | "SZ" | "NI" | "PRI" | "PGID" | "SID" | "NLWP" | "LWP" | "TGID" | "UID" | "USER" => { try { $value | into int } catch { $value } }
    "%CPU" | "%MEM" => { try { $value | into float } catch { $value } }
    "TIME" | "TIME+" => { try { parse-ps-time $value } catch { $value } }
    _ => $value
  }
}

# ---- output shaping (--output compact|wide|full) --------------------------

# Completions for the `--output` flag, each describing what the mode returns.
export def output-completer []: nothing -> any {
  {
    options: {sort: false}
    completions: [
      {value: "compact", description: "primitives only — no nested columns (default when listing)"}
      {value: "wide", description: "adds nested columns: ports, mounts, labels, … (default for one object)"}
      {value: "full", description: "the raw, unshaped Docker API response"}
    ]
  }
}

# Resolve an `--output` request into a concrete representation:
#   full    -> the raw, unshaped server response
#   wide    -> the shaped view (may include nested list/record columns)
#   compact -> the shaped view with nested columns dropped (primitives only)
# When `mode` is empty the default is compact for a list of objects (`--list`)
# and wide for a single object.
export def render-output [mode: any, raw: any, wide: any, --list]: nothing -> any {
  let m = if ($mode | is-empty) { if $list { "compact" } else { "wide" } } else { $mode }
  match $m {
    "full" => $raw
    "wide" => $wide
    "compact" => ($wide | keep-primitives)
    _ => (error make {msg: $"invalid --output '($m)': expected one of compact, wide, full"})
  }
}

# Drop nested (list/record/table) columns, keeping only primitive scalars.
# Applies field-wise to a record, element-wise to a table/list, and passes
# scalars (string/int/filesize/datetime/…) through untouched.
export def keep-primitives []: any -> any {
  let v = $in
  let t = ($v | describe)
  if (($t | str starts-with "table") or ($t | str starts-with "list")) {
    if ($v | is-empty) { return $v }
    if (($v | first | describe | str starts-with "record") == false) { return $v }  # list of scalars
    # A column is nested if *any* row holds a list/record/table there — drop it
    # table-wide so the result is a clean, uniform primitives-only table.
    let nested = ($v | columns | where {|c|
      $v | any {|row|
        let dt = ($row | get -o $c | describe)
        ($dt | str starts-with "list") or ($dt | str starts-with "record") or ($dt | str starts-with "table")
      }
    })
    if ($nested | is-empty) { $v } else { $v | reject ...$nested }
  } else if ($t | str starts-with "record") {
    $v | drop-nested-fields
  } else {
    $v
  }
}

# Record -> record keeping only primitive fields (drops list/record/table).
def drop-nested-fields []: any -> any {
  let r = $in
  if (($r | describe) | str starts-with "record") == false { return $r }
  let keep = ($r | columns | where {|c|
    let dt = ($r | get $c | describe)
    not (($dt | str starts-with "list") or ($dt | str starts-with "record") or ($dt | str starts-with "table"))
  })
  if ($keep | is-empty) { {} } else { $r | select ...$keep }
}

# Parse an ISO-8601 timestamp (as Docker *inspect* returns) into a `datetime`.
# Empty/null or Docker's zero-time sentinel -> null.
export def iso-to-datetime []: any -> any {
  let s = $in
  if ($s | is-empty) or ($s == "0001-01-01T00:00:00Z") { null } else { $s | into datetime }
}

# Parse the inspect-style port map — {"3306/tcp": [{HostIp, HostPort}], "33060/tcp": null} —
# into the same typed record list as `parse-ports`: {host_ip, host_port, container_port, proto}.
export def parse-ports-map []: any -> list {
  ($in | default {}) | items {|key bindings|
    let parts = ($key | split row "/")
    let cport = ($parts.0 | into int)
    let proto = ($parts | get 1? | default "tcp")
    if ($bindings | is-empty) {
      [{host_ip: null, host_port: null, container_port: $cport, proto: $proto}]
    } else {
      $bindings | each {|b| {host_ip: ($b.HostIp?), host_port: ($b.HostPort? | into int), container_port: $cport, proto: $proto}}
    }
  } | flatten
}
