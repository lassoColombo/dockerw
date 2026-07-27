# nu-docker-shim - structured, read-only Docker introspection for Nushell.
#
# Wraps a subset of docker's introspection subcommands with exact wrappers that
# return structured, typed, queryable data instead of text. The same
# implementation (core.nu) is exposed through two facades - pick one at import:
#
#   STANDALONE (default) - commands live under the module name, alongside real docker:
#       use nu-docker-shim          # -> `nu-docker-shim ps`, `nu-docker-shim volume ls`, …
#       module ds { export use nu-docker-shim * }   # rename the prefix to taste
#       use ds                      # -> `ds ps`, `ds volume ls`, …
#     Real `docker …` is untouched - this flavour sits beside it, it does not shadow it.
#
#   SHIM - shadows the matching docker subcommands (load with `*`, unprefixed):
#       use nu-docker-shim shim *   # -> `docker ps`, `docker volume ls`, … override real docker
#     Every OTHER `docker …` command (run, build, logs, exec, …) falls straight
#     through to the native binary, untouched. See shim.nu.
#
# Shadowed / wrapped (structured) commands - docker's flags that actually do
# something here are kept; flags with no effect on structured output (--format, …)
# are dropped. Standalone names shown; the shim prefixes each with `docker `:
#   ps · container inspect · stats · top · diff · port
#   images · image inspect · history · search
#   network ls · network inspect · volume ls · volume inspect
#   info · version · system df · inspect (generic, auto-detects type)
#
# Every structured command except `top` accepts --output (-o) compact|wide|full
# (a nu-docker-shim addition; `top` is skipped because -o would clash with `ps`'s
# own -o, forwarded through top's ps-args):
#   compact  primitives only (no nested columns)   [default when listing]
#   wide     richer, may include nested columns     [default for one object]
#   full     the raw, unshaped Docker API response
#
# Filters are exposed as discrete, completable flags - Nushell can only complete
# scalar flag values, never inside a `{record}`/`[list]` literal, so a flag per
# key is the only shape that completes (many with live socket-backed values:
# `--network`, `--volume`, `--ancestor`, `--name`, …). `ps --status running
# --network db` instead of docker's `-f status=running -f network=db`. Any key
# without its own flag goes through `--filter {record}`, the raw escape hatch.
#
# NOTE on help: each command's docstring/@example uses the canonical `docker …`
# form (it documents the real docker command it wraps). In standalone mode drop
# the `docker ` prefix (`docker ps --all` -> `ps --all` / `nu-docker-shim ps --all`).
#
# Layers:
#   client.nu   generated from spec/docker.swagger.yaml (GET-only; never hand-edited)
#   common.nu   shared helpers (socket, filters, shaping, completers)
#   *.nu        the hand-written ergonomic wrappers (core.nu assembles them)
#
# Local unix socket only: $env.DOCKER_HOST (when unix://), else the first existing
# of /var/run/docker.sock, ~/.docker/run/docker.sock, $XDG_RUNTIME_DIR/docker.sock.
# See `common docker-socket`.

export use core.nu *
export module shim.nu
