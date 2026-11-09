# Route cache clock correction patch

Base: official `8428a89d2dabc94c97d370ec607c6ca142473626` (the local
`2.6.4-8428a89d` build). Branch: `fix/monotonic-route-clock`.

## Scope and wire compatibility

Peer and foreign-network cache ages, periodic self renewal and incremental-sync
cursors use local elapsed time. The 3660-second TTL and 90-second unreachable
threshold are unchanged. Local time metadata is created only when a version is
accepted, and removed with its entry. Receipt of the same version does not renew
its TTL. Existing version acknowledgements still suppress redundant sends.

No protobuf schema, field number, timestamp encoding, route version type,
credential expiry or configuration format changes. Peer and foreign-network
`last_update` values remain real wall-clock timestamps. Incoming peer timestamps
are rebased to local wall time exactly as in the official implementation; local
monotonic receipt metadata is separate and never serialized. Connection timestamps
and session cursors are private in-memory fields, not wire fields.

The local foreign-network producer can change its wall timestamp backwards after
correction. Such a change now triggers publication too. Within a running process,
renewal uses `max(previous published version + 1, wall epoch seconds)` so existing
official receivers accept the update even after a rollback. The field still means
the same increasing version used by official version comparisons.

Cross-restart limitation: the official base initializes a new foreign-network
entry's version from wall epoch seconds, without persisted high-water state.
Restarting after a wall-clock rollback can therefore publish a lower version than
remote nodes retained. This patch preserves that initial-version behavior. It
does not add a persistent state file or claim to solve that existing cross-restart
limitation. Process-local age metadata is rebuilt on restart.

Unpatched nodes retain their own susceptibility to local clock correction. Mixed
deployment keeps the protocol compatible; a clock correction on an unpatched node
still needs separate evaluation. No running local or remote EasyTier instance is
replaced or restarted by this change.

## Suspend behavior

`peers/cache_clock.rs` exposes an injectable clock with separate elapsed and wall
time methods. Linux/Android read `CLOCK_BOOTTIME`; macOS/iOS read Darwin
`CLOCK_MONOTONIC`. These native clocks include suspended time. Windows uses
`std::time::Instant` / QPC. Native reads are checked and never require changing
system time. If a native read fails, elapsed `Instant` time continues from the
last successful reading; readings never regress. Other operating systems use
the standard `Instant` fallback. On platforms whose fallback excludes suspend,
an unavailable native clock also loses suspend accounting for that interval.

## Tests

Clock injection tests call the real cache cleanup, update and sync builders with
wall jumps of +1200, +1245, -1200 and -86400 seconds. They cover a 45-minute live
peer and foreign route, normal 61-minute expiry, 90-second unreachable cleanup,
two-hour suspend, hourly self renewal, foreign-version growth after rollback,
incremental peer/connection updates, retry before acknowledgement, suppression
after acknowledgement, and preserved wall-time timestamps.

```sh
cargo test --locked -p easytier --lib peers::peer_ospf_route::clock_tests
cargo test --locked -p easytier --lib peers::cache_clock::tests
cargo test --locked -p easytier --lib peers::peer_ospf_route::tests -- --test-threads=1
```

The dedicated GitHub Actions workflow is kept separately on
`ci/monotonic-route-clock-validation`, based on this fix branch. It is ready for
Linux and macOS native-clock checks. Windows uses the completed local 50-test
verification; the remote Windows job and repeated full OSPF suite are omitted. Local Windows native-clock tests can also be compiled independently;
the full existing Windows crate requires the upstream native build prerequisites.
Tests inject time; they never set the host's clock or touch production configs.

## Updating from upstream

Keep both remotes:

```sh
git remote -v
# origin   https://github.com/yisqiu/EasyTier.git
# upstream https://github.com/EasyTier/EasyTier.git
git fetch upstream --tags
git fetch origin
```

Rebase on a new branch so the original reviewed branch and `origin/main` remain
available. Replace `YYYYMMDD` with the upgrade date:

```sh
git switch -c fix/monotonic-route-clock-YYYYMMDD fix/monotonic-route-clock
git rebase --onto upstream/main 8428a89d2dabc94c97d370ec607c6ca142473626
# Resolve relevant conflicts, git add the resolved files, git rebase --continue.
# To abandon the attempt: git rebase --abort.
```

Recent upstream moves OSPF from `easytier/src/peers/peer_ospf_route.rs` to
`easytier-core/src/peers/route/peer_ospf_route.rs`. During that migration, port the
local metadata, injected clock and tests to the new owner; retain real wire
timestamps and credential calendar time. Check upstream's own clock fixes before
keeping this patch. A conflict-free rebase still requires the tests above and an
isolated mixed-version topology test before a production rollout.

An alternative that preserves history:

```sh
git switch -c fix/monotonic-route-clock-merge-YYYYMMDD fix/monotonic-route-clock
git merge upstream/main
```

After verification, push the new upgrade branch normally. Do not force-push main:

```sh
git push -u origin HEAD
git format-patch 8428a89d2dabc94c97d370ec607c6ca142473626..fix/monotonic-route-clock -o ../patches
```

Before pushing, inspect the staged file list and diff. Include every new clock,
test and workflow file with `git add`; publish no runtime configs, logs, secrets,
tokens or build products. Export final patches with `git format-patch` so added
files are included.
