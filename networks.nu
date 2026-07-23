# Network commands — exact wrappers of `docker network ls` / `docker network inspect`.
# Same flags as docker; structured output. `--output (-o) compact|wide|full` is a
# dockerw-specific addition.
use client.nu
use common.nu

# Network refs are completed by `common complete-network` (shared). The
# enum/known-value filters get their own static completers:
def complete-network-type []: nothing -> list<string> { [custom builtin] }
def complete-scope []: nothing -> list<string> { [swarm global local] }
def complete-network-driver []: nothing -> list<string> { [bridge host overlay macvlan ipvlan none] }

# Live label completion (comma-separated), sourced from real network labels.
def complete-network-label [context: string] {
  common complete-label $context (common harvest-labels (try {
    client networks list --unix-socket (common docker-socket)
  } catch { [] }))
}

def shape-network-summary [n: record, full_id: bool]: nothing -> record {
  {
    id: (if $full_id { $n.Id } else { $n.Id | common short-id })
    name: $n.Name
    driver: $n.Driver
    scope: $n.Scope
    internal: ($n.Internal?)
    labels: ($n.Labels?)
  }
}

def shape-network-detail [n: record]: nothing -> record {
  {
    id: ($n.Id | common short-id)
    name: $n.Name
    driver: $n.Driver
    scope: $n.Scope
    created: ($n.Created? | common iso-to-datetime)
    internal: ($n.Internal?)
    attachable: ($n.Attachable?)
    ipv6: ($n.EnableIPv6?)
    subnets: ($n.IPAM?.Config? | default [] | each {|c| {subnet: $c.Subnet?, gateway: $c.Gateway?}})
    containers: ($n.Containers? | default {} | items {|id c| {name: $c.Name?, ipv4: $c.IPv4Address?, mac: $c.MacAddress?}})
    options: ($n.Options?)
    labels: ($n.Labels?)
  }
}

# List networks as structured rows.
#
# Exact wrapper of `docker network ls`. Every filter key is its own tab-completing
# flag; anything without a dedicated flag goes through `--filter {…}`. Use
# `docker network inspect` for the full detail (subnets, connected containers, …).
@search-terms network ls list networks driver scope bridge overlay
@example "all networks" { docker network ls }
@example "user-defined bridge networks" { docker network ls --type custom --driver bridge }
@example "networks unused by any container" { docker network ls --dangling | get name }
export def "docker network ls" [
  --type: string@complete-network-type              # network type: custom | builtin
  --scope: string@complete-scope                    # scope: swarm | global | local
  --driver: string@complete-network-driver          # driver: bridge|host|overlay|macvlan|ipvlan|none
  --name: string@"common complete-network"          # name (substring match)
  --id: string@"common complete-network"            # network id
  --dangling                                        # only networks unused by any container (for =false use --filter {dangling: false})
  --label: string@complete-network-label            # label(s), comma-separated: app=web,tier=db
  --filter (-f): record = {}                        # escape hatch for filter keys without a flag
  --no-trunc                                        # don't truncate the network id
  --quiet (-q)                                       # output only ids (a list<string>)
  --output (-o): string@"common output-completer"   # shape: compact (list default) | wide | full (raw API)
]: nothing -> any {
  let sock = (common docker-socket)
  mut filters = ($filter | common add-filters {type: $type, scope: $scope, driver: $driver, name: $name, id: $id, label: (if ($label | is-empty) { null } else { $label | split row ',' })})
  if $dangling { $filters = ($filters | common add-filters {dangling: "true"}) }
  let raw = (client networks list --unix-socket $sock --filters (common build-filters $filters))
  if $quiet {
    return ($raw | each {|n| if $no_trunc { $n.Id } else { $n.Id | common short-id }})
  }
  let wide = ($raw | each {|n| shape-network-summary $n $no_trunc })
  common render-output $output $raw $wide --list
}

# Inspect one or more networks in full detail.
#
# Exact wrapper of `docker network inspect`. Returns the curated per-network detail
# — driver, scope, subnets/gateways, attached containers, options, labels — or the
# raw API response with `-o full`. A single ref returns one record; multiple refs
# return a list.
@search-terms inspect network detail subnet gateway containers
@example "inspect a network" { docker network inspect bridge }
@example "its subnets" { docker network inspect bridge | get subnets }
@example "containers attached to a network" { docker network inspect my-net | get containers }
export def "docker network inspect" [
  ...network: string@"common complete-network"      # one or more network names or ids
  --output (-o): string@"common output-completer"    # shape: wide (single-object default) | compact | full (raw API)
]: nothing -> any {
  if ($network | is-empty) { error make {msg: '"docker network inspect" requires at least 1 argument'} }
  let sock = (common docker-socket)
  let raws = ($network | each {|n| client networks get $n --unix-socket $sock })
  let wides = ($raws | each {|r| shape-network-detail $r })
  if (($network | length) == 1) {
    common render-output $output ($raws | first) ($wides | first)
  } else {
    common render-output $output $raws $wides
  }
}

# --- alias: docker's `list` form of `network ls` ---
export alias "docker network list" = docker network ls
