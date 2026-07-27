# System commands — exact wrappers of `docker info`, `docker version`, `docker system df`.
# Same flags as docker; structured output.
use client.nu
use common.nu

# Daemon-wide information (the `GET /info` record).
#
# Exact wrapper of `docker info`. This record is uncurated, so `wide` and `full`
# are identical; `compact` drops the nested list/record fields, leaving the scalar
# summary. Handy for pulling a single field with `get`.
@search-terms info system daemon engine
@example "the full info record" { docker info }
@example "number of running containers" { docker info | get ContainersRunning }
@example "just the scalar fields" { docker info -o compact }
export def "info" [
  --output (-o): string@"common output-completer"   # shape: wide (default, == full) | compact (scalars only) | full
]: nothing -> any {
  let raw = (client info get-system --unix-socket (common docker-socket))
  common render-output $output $raw $raw
}

# Daemon (server-side) version details (the `GET /version` record).
#
# Exact wrapper of `docker version` — server side only (no client section). The
# record is uncurated, so `wide` == `full`; `compact` keeps only scalar fields.
@search-terms version api build server engine
@example "version details" { docker version }
@example "the negotiated API version" { docker version | get ApiVersion }
export def "version" [
  --output (-o): string@"common output-completer"   # shape: wide (default, == full) | compact (scalars only) | full
]: nothing -> any {
  let raw = (client version version-system --unix-socket (common docker-socket))
  common render-output $output $raw $raw
}

# Disk-usage summary across images, containers, volumes, and build cache.
#
# Exact wrapper of `docker system df`. One row per resource type with `total`,
# `active`, and reclaimable `size` (a `filesize`) — the structured equivalent of
# the CLI's summary table.
@search-terms system df disk usage space reclaim prune
@example "disk-usage summary" { docker system df }
@example "total space used by all types" { docker system df | get size | math sum }
export def "system df" [
  --output (-o): string@"common output-completer"   # shape: compact (default) | wide | full (raw usage record)
]: nothing -> any {
  let df = (client system-df get-data-usage --unix-socket (common docker-socket))
  let images = ($df.Images? | default [])
  let containers = ($df.Containers? | default [])
  let volumes = ($df.Volumes? | default [])
  let cache = ($df.BuildCache? | default [])
  let wide = [
    {
      type: "Images"
      total: ($images | length)
      active: ($images | where Containers > 0 | length)
      size: (($df.LayersSize? | default 0) | into filesize)
    }
    {
      type: "Containers"
      total: ($containers | length)
      active: ($containers | where State == "running" | length)
      size: ($containers | reduce --fold 0 {|it acc| $acc + ($it.SizeRw? | default 0)} | into filesize)
    }
    {
      type: "Local Volumes"
      total: ($volumes | length)
      active: ($volumes | where {|v| ($v.UsageData?.RefCount? | default 0) > 0} | length)
      size: ($volumes | reduce --fold 0 {|it acc| $acc + ($it.UsageData?.Size? | default 0)} | into filesize)
    }
    {
      type: "Build Cache"
      total: ($cache | length)
      active: ($cache | where InUse | length)
      size: ($cache | reduce --fold 0 {|it acc| $acc + ($it.Size? | default 0)} | into filesize)
    }
  ]
  common render-output $output $df $wide --list
}

# --- alias: docker's `system` form of `info` ---
export alias "system info" = info
