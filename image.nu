# Image commands — exact wrappers of `docker images`, `docker image inspect`,
# `docker history`, `docker search`. Same flags as docker; structured output.
# `--output (-o) compact|wide|full` is a nu-docker-shim-specific addition on the two
# resource commands (`images` / `image inspect`).
use client.nu
use common.nu

# --- completions ---
# Image refs are completed by `common complete-image` (shared).

# Live label completion (comma-separated), sourced from real image labels.
def complete-image-label [context: string] {
  common complete-label $context (common harvest-labels (try {
    client images-json list --unix-socket (common docker-socket)
  } catch { [] }))
}

# --- shapers ---

# Shape rows of the image LIST endpoint — one row per repository:tag.
def shape-image-summary [img: record, full_id: bool, with_digest: bool]: nothing -> list {
  let repo_tags = ($img.RepoTags? | default [])
  let tags = if ($repo_tags | is-empty) { ["<none>:<none>"] } else { $repo_tags }
  let digest = if $with_digest { ($img.RepoDigests? | default [] | get 0? | default "" | split row '@' | last) } else { null }
  $tags | each {|rt|
    let segs = ($rt | split row ":")
    let row = {
      repository: ($segs | drop 1 | str join ":")
      tag: ($segs | last)
      id: (if $full_id { $img.Id } else { $img.Id | common short-id })
      created: ($img.Created | common epoch-to-datetime)
      size: ($img.Size | into filesize)
      labels: ($img.Labels?)
    }
    if $with_digest { $row | insert digest $digest } else { $row }
  }
}

# Shape an image INSPECT response (the "wide" detail view).
def shape-image-detail [i: record]: nothing -> record {
  {
    id: ($i.Id | common short-id)
    repo_tags: ($i.RepoTags?)
    repo_digests: ($i.RepoDigests?)
    created: ($i.Created? | common iso-to-datetime)
    size: ($i.Size? | into filesize)
    os: ($i.Os?)
    architecture: ($i.Architecture?)
    author: ($i.Author?)
    docker_version: ($i.DockerVersion?)
    parent: ($i.Parent? | common short-id)
    cmd: ($i.Config?.Cmd?)
    entrypoint: ($i.Config?.Entrypoint?)
    env: ($i.Config?.Env?)
    exposed_ports: ($i.Config?.ExposedPorts? | default {} | columns)
    labels: ($i.Config?.Labels?)
  }
}

# --- commands ---

# List images, one row per repository:tag.
#
# Exact wrapper of `docker images`. `created` is a `datetime` and `size` a
# `filesize`, so you can sort and sum them directly. An image with several tags
# yields one row per tag; untagged layers show as `<none>:<none>`. An optional
# `REPOSITORY[:TAG]` positional narrows to matching references. Filter keys are
# discrete flags; use `--filter {…}` for anything without a dedicated flag.
@search-terms images ls list repository tag size dangling
@example "all images" { docker images }
@example "images over 1 GB, largest first" { docker images | where size > 1gb | sort-by size --reverse }
@example "one repository" { docker images postgres }
@example "dangling (untagged) images and their size" { docker images --dangling | select repository tag size }
export def "images" [
  repository?: string@"common complete-image"       # optional REPOSITORY[:TAG] to narrow to
  --all (-a)                                         # include intermediate layers (default: hidden)
  --digests                                          # add a `digest` column
  --dangling                                         # only untagged images (for dangling=false use --filter {dangling: false})
  --before: string@"common complete-image"           # only images created before this one
  --since: string@"common complete-image"            # only images created after this one
  --label: string@complete-image-label               # label(s), comma-separated: app=web,tier=db
  --filter (-f): record = {}                         # escape hatch, e.g. {reference: "postgres:*"}
  --no-trunc                                          # don't truncate the image id
  --quiet (-q)                                        # output only ids (a list<string>, deduped)
  --output (-o): string@"common output-completer"    # shape: compact (list default) | wide | full (raw API)
]: nothing -> any {
  let sock = (common docker-socket)
  mut filters = ($filter | common add-filters {before: $before, since: $since, label: (if ($label | is-empty) { null } else { $label | split row ',' })})
  if $dangling { $filters = ($filters | common add-filters {dangling: "true"}) }
  if ($repository | is-not-empty) { $filters = ($filters | common add-filters {reference: $repository}) }
  let raw = (client images-json list --unix-socket $sock --all $all --digests $digests --filters (common build-filters $filters))
  if $quiet {
    return ($raw | each {|img| if $no_trunc { $img.Id } else { $img.Id | common short-id }} | uniq)
  }
  let wide = ($raw | each {|img| shape-image-summary $img $no_trunc $digests } | flatten)
  common render-output $output $raw $wide --list
}

# Inspect one or more images in full detail.
#
# Exact wrapper of `docker image inspect`. Returns the curated per-image detail —
# repo tags/digests, os/architecture, config (cmd, entrypoint, env, exposed ports),
# and labels — or the raw API response with `-o full`. A single ref returns one
# record; multiple refs return a list.
@search-terms inspect image detail config layers digest
@example "inspect an image" { docker image inspect postgres:16 }
@example "its exposed ports" { docker image inspect nginx | get exposed_ports }
export def "image inspect" [
  ...image: string@"common complete-image"          # one or more image names or ids
  --output (-o): string@"common output-completer"    # shape: wide (single-object default) | compact | full (raw API)
]: nothing -> any {
  if ($image | is-empty) { error make {msg: '"docker image inspect" requires at least 1 argument'} }
  let sock = (common docker-socket)
  let raws = ($image | each {|i| client images-json get $i --unix-socket $sock })
  let wides = ($raws | each {|r| shape-image-detail $r })
  if (($image | length) == 1) {
    common render-output $output ($raws | first) ($wides | first)
  } else {
    common render-output $output $raws $wides
  }
}

# Show the build history (layers) of an image.
#
# Exact wrapper of `docker history`. One row per layer, newest first: `created`
# is a `datetime`, `size` a `filesize`, `created_by` the build instruction
# (whitespace-collapsed unless `--no-trunc`).
@search-terms history layers build image size
@example "layer history" { docker history postgres:16 }
@example "total image size from layers" { docker history nginx | get size | math sum }
export def "history" [
  image: string@"common complete-image"             # the image to inspect
  --no-trunc                                         # don't truncate ids or the build instruction
  --quiet (-q)                                        # output only layer ids (a list<string>)
  --output (-o): string@"common output-completer"    # shape: compact (default) | wide | full (raw layers)
]: nothing -> any {
  let raw = (client images-history get $image --unix-socket (common docker-socket))
  if $quiet {
    return ($raw | each {|h| if $no_trunc { $h.Id } else { $h.Id | common short-id }})
  }
  let wide = ($raw | each {|h| {
    id: (if $no_trunc { $h.Id } else if (($h.Id | default "") | str starts-with "sha256:") { $h.Id | common short-id } else { $h.Id })
    created: ($h.Created | common epoch-to-datetime)
    created_by: (if $no_trunc { $h.CreatedBy } else { $h.CreatedBy | str replace -a -r '\s+' ' ' | str trim })
    size: ($h.Size | into filesize)
    comment: $h.Comment
  }})
  common render-output $output $raw $wide --list
}

# Search Docker Hub for images.
#
# Exact wrapper of `docker search` — the one shadowed command that hits the
# network (Docker Hub) rather than the local daemon. Rows are
# `{name, stars, official, description}`, sorted by stars descending.
@search-terms search hub find registry stars official
@example "search Docker Hub" { docker search nginx }
@example "popular official images only" { docker search postgres --official --stars 100 --limit 10 }
export def "search" [
  term: string                                       # the term to search Docker Hub for
  --stars: int                                       # minimum star count
  --official                                         # only official images (is-official=true)
  --automated                                        # only automated builds (is-automated=true)
  --filter (-f): record = {}                         # escape hatch for other search filter keys
  --limit: int                                        # cap the number of results
  --no-trunc                                          # don't truncate the description column
  --output (-o): string@"common output-completer"    # shape: compact (default) | wide | full (raw results)
]: nothing -> any {
  mut filters = ($filter | common add-filters {stars: $stars})
  if $official { $filters = ($filters | common add-filters {"is-official": "true"}) }
  if $automated { $filters = ($filters | common add-filters {"is-automated": "true"}) }
  let raw0 = (client images-search list --unix-socket (common docker-socket) --term $term --filters (common build-filters $filters))
  let raw = if ($limit != null) { $raw0 | first $limit } else { $raw0 }
  let wide = ($raw | each {|r| {
    name: $r.name
    stars: $r.star_count
    official: $r.is_official
    description: (if $no_trunc { $r.description } else { $r.description | str substring 0..<50 })
  }} | sort-by stars --reverse)
  common render-output $output $raw $wide --list
}

# --- aliases: docker's `image` subcommand forms of the commands above ---
export alias "image ls" = images
export alias "image list" = images
export alias "image history" = history
