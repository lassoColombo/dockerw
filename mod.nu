# nu-docker-shim — structured, read-only Docker introspection for Nushell.
#
# This module *shadows* a handful of `docker` subcommands with exact wrappers
# that return structured, typed, queryable data instead of text. Load it with:
#
#     use /path/to/nu-docker-shim *        # note the `*` — the defs must be unprefixed
#
# so `def "docker ps"`, `def "docker images"`, … override the real subcommands.
# Every OTHER `docker …` command (run, build, logs, exec, …) is untouched and
# falls straight through to the native `docker` binary — with its own native
# completion (via your external completer, e.g. carapace).
#
# Shadowed (structured) commands — docker's flags that actually do something here
# (flags with no effect on structured output, like --format, are dropped):
#   docker ps · docker container inspect · docker stats · docker top · docker diff
#   docker images · docker image inspect · docker history · docker search
#   docker network ls · docker network inspect · docker volume ls · docker volume inspect
#   docker info · docker version · docker system df
#
# Every structured command except `docker top` accepts --output (-o) compact|wide|full
# (a nu-docker-shim-specific flag with real function; `top` is skipped because -o would clash
# with `ps`'s own -o option passed through its ps-args):
#   compact  primitives only (no nested columns)   [default when listing]
#   wide     richer, may include nested columns     [default for one object]
#   full     the raw, unshaped Docker API response
#
# Filters are exposed as discrete, completable flags — Nushell can only complete
# scalar flag values, never inside a `{record}`/`[list]` literal, so a flag per
# key is the only shape that completes (many with live socket-backed values:
# `--network`, `--volume`, `--ancestor`, `--name`, …). `docker ps --status running
# --network db` instead of docker's `-f status=running -f network=db`. Any key
# without its own flag goes through `--filter {record}`, the raw escape hatch.
#
# Layers:
#   client.nu   generated from spec/docker.swagger.yaml (GET-only; never hand-edited)
#   *.nu        this hand-written ergonomic wrapper
#
# Socket resolved from $env.DOCKER_HOST (when unix://) or /var/run/docker.sock.

use common.nu

export use containers.nu *
export use images.nu *
export use networks.nu *
export use volumes.nu *
export use system.nu *

# Inspect any object, auto-detecting its type.
#
# Generic wrapper of `docker inspect`. For each ref it tries container -> image ->
# network -> volume in turn and returns the same curated detail as the matching
# `docker <type> inspect`. Anything else docker can inspect (plugins, swarm
# objects, …) falls back to `^docker inspect` parsed into a structured record.
# A single ref returns one record; multiple refs return a list. Prefer the
# type-specific commands when you know the type — they also complete the ref.
@search-terms inspect detail object generic auto-detect
@example "inspect whatever this ref is" { docker inspect redis }
@example "mix types in one call" { docker inspect redis postgres:16 bridge }
@example "raw API response" { docker inspect redis -o full }
export def "docker inspect" [
  ...ref: string                                    # one or more object names/ids, of any type
  --output (-o): string@"common output-completer"   # shape: wide (single-object default) | compact | full (raw API)
]: nothing -> any {
  if ($ref | is-empty) { error make {msg: '"docker inspect" requires at least 1 argument'} }
  let mode = ($output | default "wide")
  let results = ($ref | each {|r|
    try { docker container inspect $r -o $mode } catch {
    try { docker image inspect $r -o $mode } catch {
    try { docker network inspect $r -o $mode } catch {
    try { docker volume inspect $r -o $mode } catch {
      let raw = (^docker inspect $r | from json | first)
      common render-output $output $raw $raw
    }}}}
  })
  if (($ref | length) == 1) { $results | first } else { $results }
}
