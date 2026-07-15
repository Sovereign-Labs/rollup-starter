#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Find the first rollup slot whose state root differs between two nodes.

Usage:
  find-state-root-divergence.sh LOCAL_URL SECONDARY_URL [FROM_SLOT] [TO_SLOT]

Arguments:
  LOCAL_URL      Updated node, for example http://127.0.0.1:12346
  SECONDARY_URL  Known-good node, usually http://10.0.2.15:8081 through nginx
  FROM_SLOT      A slot known to match. Defaults to 0.
  TO_SLOT        A slot known to differ. Defaults to the lower node tip.

The script compares compact ledger responses with a binary search, then saves
full responses for the first mismatching slot under /tmp. FROM_SLOT must be
available from both nodes and should precede the upgrade/replay boundary.

The common finalized tip is used by default. Set HEAD=latest to include pending
slots when no finalized mismatch exists yet.
EOF
}

die() {
    echo "error: $*" >&2
    exit 1
}

if [[ ${1:-} == "-h" || ${1:-} == "--help" ]]; then
    usage
    exit 0
fi

[[ $# -ge 2 && $# -le 4 ]] || {
    usage >&2
    exit 2
}

for command in curl jq; do
    command -v "$command" >/dev/null 2>&1 || die "required command not found: $command"
done

LOCAL_URL=${1%/}
SECONDARY_URL=${2%/}
FROM_SLOT=${3:-0}
TO_SLOT=${4:-}
HEAD=${HEAD:-finalized}

[[ $FROM_SLOT =~ ^[0-9]+$ ]] || die "FROM_SLOT must be a non-negative integer"
[[ $HEAD == "finalized" || $HEAD == "latest" ]] || die "HEAD must be finalized or latest"
if [[ -n $TO_SLOT ]]; then
    [[ $TO_SLOT =~ ^[0-9]+$ ]] || die "TO_SLOT must be a non-negative integer"
fi

fetch_slot() {
    local base_url=$1
    local slot=$2
    local children=${3:-0}

    curl --fail-with-body --silent --show-error \
        --connect-timeout 5 --max-time 30 --retry 2 \
        "${base_url}/ledger/slots/${slot}?children=${children}"
}

slot_summary() {
    local base_url=$1
    local slot=$2
    local response

    response=$(fetch_slot "$base_url" "$slot" 0) || return 2
    jq --exit-status --raw-output \
        '[.number, .hash, .state_root] | @tsv' <<<"$response" || return 2
}

load_comparison() {
    local slot=$1
    local local_summary secondary_summary

    local_summary=$(slot_summary "$LOCAL_URL" "$slot") || return 2
    secondary_summary=$(slot_summary "$SECONDARY_URL" "$slot") || return 2

    IFS=$'\t' read -r LOCAL_NUMBER LOCAL_HASH LOCAL_ROOT <<<"$local_summary"
    IFS=$'\t' read -r SECONDARY_NUMBER SECONDARY_HASH SECONDARY_ROOT <<<"$secondary_summary"

    [[ $LOCAL_NUMBER == "$slot" ]] || {
        echo "local node returned slot $LOCAL_NUMBER for requested slot $slot" >&2
        return 2
    }
    [[ $SECONDARY_NUMBER == "$slot" ]] || {
        echo "secondary node returned slot $SECONDARY_NUMBER for requested slot $slot" >&2
        return 2
    }
    if [[ $LOCAL_HASH != "$SECONDARY_HASH" ]]; then
        echo "slot $slot has different DA hashes; this is not only a state-root divergence" >&2
        echo "  local:     $LOCAL_HASH" >&2
        echo "  secondary: $SECONDARY_HASH" >&2
        return 2
    fi
}

roots_match() {
    local slot=$1
    load_comparison "$slot" || return 2
    [[ $LOCAL_ROOT == "$SECONDARY_ROOT" ]]
}

print_root_parts() {
    local label=$1
    local root=${2#0x}

    if [[ $root =~ ^[[:xdigit:]]{128}$ ]]; then
        echo "$label user root:   0x${root:0:64}"
        echo "$label kernel root: 0x${root:64:64}"
    fi
}

latest_number() {
    local summary
    summary=$(slot_summary "$1" "$HEAD") || return 1
    cut -f1 <<<"$summary"
}

LOCAL_TIP=$(latest_number "$LOCAL_URL") || die "failed to query local node $HEAD tip"
SECONDARY_TIP=$(latest_number "$SECONDARY_URL") || die "failed to query secondary node $HEAD tip"

if [[ -z $TO_SLOT ]]; then
    if (( LOCAL_TIP < SECONDARY_TIP )); then
        TO_SLOT=$LOCAL_TIP
    else
        TO_SLOT=$SECONDARY_TIP
    fi
fi

(( FROM_SLOT <= TO_SLOT )) || die "FROM_SLOT must not exceed TO_SLOT"
(( TO_SLOT <= LOCAL_TIP )) || die "TO_SLOT exceeds local tip $LOCAL_TIP"
(( TO_SLOT <= SECONDARY_TIP )) || die "TO_SLOT exceeds secondary tip $SECONDARY_TIP"

echo "local $HEAD tip:     $LOCAL_TIP"
echo "secondary $HEAD tip: $SECONDARY_TIP"
echo "search range:        [$FROM_SLOT, $TO_SLOT]"

if roots_match "$FROM_SLOT"; then
    echo "slot $FROM_SLOT matches"
else
    status=$?
    (( status == 1 )) || die "failed to compare slot $FROM_SLOT"
    if (( FROM_SLOT == 0 )); then
        TO_SLOT=0
    else
        die "FROM_SLOT $FROM_SLOT already differs; rerun with an earlier known-matching slot"
    fi
fi

if (( TO_SLOT != 0 )); then
    if roots_match "$TO_SLOT"; then
        echo "no state-root divergence found through slot $TO_SLOT"
        exit 0
    else
        status=$?
        (( status == 1 )) || die "failed to compare slot $TO_SLOT"
        echo "slot $TO_SLOT differs"
    fi

    low=$FROM_SLOT
    high=$TO_SLOT
    while (( high - low > 1 )); do
        middle=$((low + (high - low) / 2))
        if roots_match "$middle"; then
            echo "slot $middle matches"
            low=$middle
        else
            status=$?
            (( status == 1 )) || die "failed to compare slot $middle"
            echo "slot $middle differs"
            high=$middle
        fi
    done
    TO_SLOT=$high
fi

FIRST_MISMATCH=$TO_SLOT
load_comparison "$FIRST_MISMATCH" || die "failed to reload first mismatching slot"

echo
echo "first mismatching slot: $FIRST_MISMATCH"
echo "slot hash:              $LOCAL_HASH"
echo "local state root:       $LOCAL_ROOT"
echo "secondary state root:   $SECONDARY_ROOT"
print_root_parts "local" "$LOCAL_ROOT"
print_root_parts "secondary" "$SECONDARY_ROOT"

if (( FIRST_MISMATCH > 0 )); then
    PREVIOUS_SLOT=$((FIRST_MISMATCH - 1))
    load_comparison "$PREVIOUS_SLOT" || die "failed to compare preceding slot $PREVIOUS_SLOT"
    echo "preceding slot:          $PREVIOUS_SLOT"
    echo "preceding state root:    $LOCAL_ROOT"
fi

OUTPUT_DIR=${OUTPUT_DIR:-$(mktemp -d /tmp/rollup-root-divergence.XXXXXX)}
mkdir -p "$OUTPUT_DIR"

fetch_slot "$LOCAL_URL" "$FIRST_MISMATCH" 1 >"$OUTPUT_DIR/updated.json"
fetch_slot "$SECONDARY_URL" "$FIRST_MISMATCH" 1 >"$OUTPUT_DIR/secondary.json"

jq --sort-keys 'del(.state_root, .finality_status)' \
    "$OUTPUT_DIR/updated.json" >"$OUTPUT_DIR/updated.normalized.json"
jq --sort-keys 'del(.state_root, .finality_status)' \
    "$OUTPUT_DIR/secondary.json" >"$OUTPUT_DIR/secondary.normalized.json"

diff -u \
    "$OUTPUT_DIR/updated.normalized.json" \
    "$OUTPUT_DIR/secondary.normalized.json" \
    >"$OUTPUT_DIR/non-root.diff" || true

echo "full responses saved in: $OUTPUT_DIR"
if [[ -s $OUTPUT_DIR/non-root.diff ]]; then
    echo "non-root response differences: $OUTPUT_DIR/non-root.diff"
else
    echo "the full responses differ only in state_root/finality_status"
fi
