# Volume commands — exact wrappers of `docker volume ls` / `docker volume inspect`.
# Same flags as docker; structured output. `--output (-o) compact|wide|full` is a
# dockerw-specific addition.
use client.nu
use common.nu

# Volume refs are completed by `common complete-volume` (shared).

# Live label completion (comma-separated), sourced from real volume labels.
def complete-volume-label [context: string] {
  common complete-label $context (common harvest-labels (try {
    client volumes list --unix-socket (common docker-socket) | get -o Volumes | default []
  } catch { [] }))
}

def shape-volume-summary [v: record]: nothing -> record {
  {
    driver: $v.Driver
    name: $v.Name
    mountpoint: $v.Mountpoint
    scope: $v.Scope
    labels: ($v.Labels?)
  }
}

def shape-volume-detail [v: record]: nothing -> record {
  {
    name: $v.Name
    driver: $v.Driver
    scope: $v.Scope
    mountpoint: $v.Mountpoint
    created: ($v.CreatedAt? | common iso-to-datetime)
    labels: ($v.Labels?)
    options: ($v.Options?)
  }
}

# List volumes as structured rows.
#
# Exact wrapper of `docker volume ls`. Filter keys are discrete tab-completing
# flags; anything without one goes through `--filter {…}`. Use
# `docker volume inspect` for the full detail (mountpoint, options, timestamps).
@search-terms volume ls list volumes driver dangling mountpoint
@example "all volumes" { docker volume ls }
@example "volumes unused by any container" { docker volume ls --dangling | get name }
@example "local-driver volumes named like data" { docker volume ls --driver local --name data }
export def "docker volume ls" [
  --dangling                                        # only volumes unused by any container (for =false use --filter {dangling: false})
  --driver: string                                  # volume driver (e.g. local)
  --name: string@"common complete-volume"           # name (substring match)
  --label: string@complete-volume-label             # label(s), comma-separated: app=web,tier=db
  --filter (-f): record = {}                        # escape hatch for filter keys without a flag
  --quiet (-q)                                        # output only names (a list<string>)
  --output (-o): string@"common output-completer"   # shape: compact (list default) | wide | full (raw API)
]: nothing -> any {
  let sock = (common docker-socket)
  mut filters = ($filter | common add-filters {driver: $driver, name: $name, label: (if ($label | is-empty) { null } else { $label | split row ',' })})
  if $dangling { $filters = ($filters | common add-filters {dangling: "true"}) }
  let raw = ((client volumes list --unix-socket $sock --filters (common build-filters $filters)).Volumes | default [])
  if $quiet {
    return ($raw | each {|v| $v.Name })
  }
  let wide = ($raw | each {|v| shape-volume-summary $v })
  common render-output $output $raw $wide --list
}

# Inspect one or more volumes in full detail.
#
# Exact wrapper of `docker volume inspect`. Returns the curated per-volume detail —
# driver, scope, mountpoint, `created` (`datetime`), options, labels — or the raw
# API response with `-o full`. A single ref returns one record; multiple refs
# return a list.
@search-terms inspect volume detail mountpoint driver options
@example "inspect a volume" { docker volume inspect devutils_dumbo-postgres-data }
@example "just the mountpoint" { docker volume inspect my-vol | get mountpoint }
export def "docker volume inspect" [
  ...volume: string@"common complete-volume"        # one or more volume names
  --output (-o): string@"common output-completer"    # shape: wide (single-object default) | compact | full (raw API)
]: nothing -> any {
  if ($volume | is-empty) { error make {msg: '"docker volume inspect" requires at least 1 argument'} }
  let sock = (common docker-socket)
  let raws = ($volume | each {|v| client volumes get $v --unix-socket $sock })
  let wides = ($raws | each {|r| shape-volume-detail $r })
  if (($volume | length) == 1) {
    common render-output $output ($raws | first) ($wides | first)
  } else {
    common render-output $output $raws $wides
  }
}

# --- alias: docker's `list` form of `volume ls` ---
export alias "docker volume list" = docker volume ls
