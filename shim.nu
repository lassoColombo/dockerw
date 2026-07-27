# SHIM facade - shadows the matching `docker …` subcommands with the structured
# core commands. Load with the `*` so the defs enter scope unprefixed:
#
#     use nu-docker-shim shim *
#     docker ps            # structured, shadows real docker
#     docker --version     # falls straight through to the native docker binary
#
# Nushell resolves the longest internal command name first, so every `docker …`
# command NOT listed below (run, build, logs, exec, pull, …) falls through to the
# external `docker` untouched - with its own native completion (carapace, …).
#
# These are `export alias`es onto the core commands (imported prefixed, so the
# core names stay internal and never leak into your scope). Aliases inherit the
# target def's full signature, flags, and custom completers - so
# `docker ps --status <TAB>` completes natively. Each docker spelling maps
# straight to its canonical core command (no alias-to-alias chains).

use core.nu

# --- top-level forms ---
export alias "docker ps" = core ps
export alias "docker stats" = core stats
export alias "docker top" = core top
export alias "docker diff" = core diff
export alias "docker port" = core port
export alias "docker images" = core images
export alias "docker history" = core history
export alias "docker search" = core search
export alias "docker info" = core info
export alias "docker version" = core version
export alias "docker inspect" = core inspect

# --- object-subcommand `inspect` / `ls` forms ---
export alias "docker container inspect" = core container inspect
export alias "docker image inspect" = core image inspect
export alias "docker network ls" = core network ls
export alias "docker network inspect" = core network inspect
export alias "docker volume ls" = core volume ls
export alias "docker volume inspect" = core volume inspect
export alias "docker system df" = core system df

# --- docker's alternate object-subcommand spellings ---
export alias "docker container ls" = core ps
export alias "docker container list" = core ps
export alias "docker container ps" = core ps
export alias "docker container stats" = core stats
export alias "docker container top" = core top
export alias "docker container diff" = core diff
export alias "docker container port" = core port
export alias "docker image ls" = core images
export alias "docker image list" = core images
export alias "docker image history" = core history
export alias "docker network list" = core network ls
export alias "docker volume list" = core volume ls
export alias "docker system info" = core info
