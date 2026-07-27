# nu-docker-shim transport - delegate to the docker CLI via a socat bridge.
#
# We run  `socat UNIX-LISTEN:<sock>,fork EXEC:'docker system dial-stdio'`  once and
# send every request with nushell's native `http get --unix-socket <sock>`. socat
# proxies each connection to `docker system dial-stdio`, so docker owns all
# transport and auth (active `docker context` / $DOCKER_HOST / TLS certs / `ssh://`)
# and nushell's HTTP client owns all HTTP framing. nu-docker-shim re-implements
# neither - no hand-rolled HTTP, no bidirectional-stdio shell choreography.
#
# The bridge is keyed by the docker target ($DOCKER_HOST + $DOCKER_CONTEXT): pointing
# at a different daemon via env starts a separate bridge; switching via
# `docker context use` is picked up automatically, because each connection re-execs
# `docker system dial-stdio`, which re-reads ~/.docker/config.json. One socat per
# target is started lazily and reused (across commands and sessions), like an ssh
# ControlMaster; it lives until killed or reboot. The socket is created mode 0600
# under $XDG_RUNTIME_DIR (falling back to $TMPDIR) - it proxies to the daemon, so it
# must not be world-connectable.
#
# Nothing here is public API; `client.nu` calls `request` (not named `get`, which
# would shadow nushell's builtin `get` used throughout this file).

# Perform one bodyless GET, returning the `http get --full` record {status, headers,
# body} that client.nu's handle-response expects. Ensures the bridge is up first; if
# the bridge died/wedged between the liveness check and the request (transport error,
# not an HTTP 4xx/5xx - those come back via --allow-errors), it is rebuilt once.
export def request [url: string, headers: record, raw: bool, timeout: duration]: nothing -> record {
  let sock = (bridge-socket)
  try {
    http get --headers $headers --full --allow-errors --max-time $timeout --raw=$raw --unix-socket $sock $url
  } catch {
    ^pkill -f $"UNIX-LISTEN:($sock)" | complete | ignore
    rm --force $sock
    let sock2 = (bridge-socket)
    http get --headers $headers --full --allow-errors --max-time $timeout --raw=$raw --unix-socket $sock2 $url
  }
}

# Path to a ready bridge socket for the current docker target, starting socat if needed.
def bridge-socket []: nothing -> string {
  let dir = ($env | get -o XDG_RUNTIME_DIR | default ($env | get -o TMPDIR | default "/tmp"))
  let host = ($env | get -o DOCKER_HOST | default "")
  let ctx = ($env | get -o DOCKER_CONTEXT | default "")
  let sig = ($"($host)|($ctx)" | hash sha256 | str substring 0..15)
  let sock = ($dir | path join $"nu-docker-shim-($sig).sock")
  if (bridge-live $sock) { return $sock }
  start-bridge $sock
  $sock
}

# Is a socat bridge currently listening on $sock? (socket file present AND a socat
# process bound to it - a bare leftover socket file with no socat is treated as dead.)
def bridge-live [sock: string]: nothing -> bool {
  if not ($sock | path exists) { return false }
  (^pgrep -f $"UNIX-LISTEN:($sock)" | complete | get stdout | str trim | is-not-empty)
}

# Start (or restart) the socat bridge for $sock. A `mkdir` lock (atomic via the
# external mkdir) makes concurrent cold-starts - e.g. `stats`' par-each - cooperate:
# one launches socat, the rest wait for it to answer. A stale lock (holder died
# before the bridge came up) is cleared and retried, bounded.
def start-bridge [sock: string]: nothing -> nothing {
  if (which socat | is-empty) {
    error make --unspanned { msg: "nu-docker-shim: `socat` not found. Install it (e.g. `brew install socat`) - it bridges to `docker system dial-stdio`." }
  }
  let lock = $"($sock).lock"
  mut tries = 0
  loop {
    $tries += 1
    if ((^mkdir $lock | complete).exit_code == 0) {
      ^bash -c $'socat UNIX-LISTEN:($sock),fork,unlink-early,mode=0600 "EXEC:docker system dial-stdio" >/dev/null 2>&1 &'
      let ok = (wait-ready $sock)
      rm --recursive --force $lock
      if $ok { return }
      error make --unspanned { msg: $"nu-docker-shim: socat bridge did not come up at ($sock). Is docker reachable? Try `docker version`." }
    }
    # another call holds the lock: wait for the bridge it is bringing up
    if (wait-ready $sock) { return }
    if $tries >= 3 {
      error make --unspanned { msg: $"nu-docker-shim: socat bridge start timed out at ($sock)." }
    }
    rm --recursive --force $lock   # stale lock - clear and retry
  }
}

# Wait until the bridge actually answers HTTP, by polling docker's /_ping (~2s cap).
# This closes the cold-start race where the socket file exists but socat is not yet
# accepting - which otherwise surfaces as an intermittent "Connection refused". A
# successful probe also validates the whole chain (socat -> dial-stdio -> daemon).
def wait-ready [sock: string]: nothing -> bool {
  mut i = 0
  while $i < 50 {
    let ok = (try { http get --max-time 2sec --unix-socket $sock "http://localhost/_ping" | ignore; true } catch { false })
    if $ok { return true }
    sleep 40ms
    $i += 1
  }
  false
}
