# Core implementation of the structured Docker-introspection commands, with
# facade-neutral names (`ps`, `container inspect`, `network ls`, `volume ls`, …).
#
# This is the single source of truth for the command set. The two public facades
# are thin wrappers over it:
#   mod.nu   the standalone module (default import) — `nu-docker-shim ps`, and the
#            nested `shim` submodule
#   shim.nu  the docker-shadowing facade — `docker ps`, … via aliases onto these
#
# Never `use` this file directly; import one of the facades. See mod.nu's header.

use common.nu

export use containers.nu *
export use image.nu *   # singular: a module file can't export a command named the same as itself (`images`)
export use networks.nu *
export use volumes.nu *
export use system.nu *

# Inspect any object, auto-detecting its type.
#
# Generic wrapper of `docker inspect`. For each ref it tries container -> image ->
# network -> volume in turn and returns the same curated detail as the matching
# `<type> inspect`. Anything else docker can inspect (plugins, swarm objects, …)
# falls back to `^docker inspect` parsed into a structured record. A single ref
# returns one record; multiple refs return a list. Prefer the type-specific
# commands when you know the type — they also complete the ref.
@search-terms inspect detail object generic auto-detect
@example "inspect whatever this ref is" { docker inspect redis }
@example "mix types in one call" { docker inspect redis postgres:16 bridge }
@example "raw API response" { docker inspect redis -o full }
export def "inspect" [
  ...ref: string                                    # one or more object names/ids, of any type
  --output (-o): string@"common output-completer"   # shape: wide (single-object default) | compact | full (raw API)
]: nothing -> any {
  if ($ref | is-empty) { error make {msg: '"inspect" requires at least 1 argument'} }
  let mode = ($output | default "wide")
  let results = ($ref | each {|r|
    try { container inspect $r -o $mode } catch {
    try { image inspect $r -o $mode } catch {
    try { network inspect $r -o $mode } catch {
    try { volume inspect $r -o $mode } catch {
      let raw = (^docker inspect $r | from json | first)
      common render-output $output $raw $raw
    }}}}
  })
  if (($ref | length) == 1) { $results | first } else { $results }
}
