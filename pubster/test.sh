#!/usr/bin/env bash
# pubster integration test suite — 6 test cases
set -uo pipefail

cd "$(dirname "$0")"

SERVER="./target/debug/server"
CLIENT="./target/debug/client"

SERVER_PID=""
PASS=0
FAIL=0

# ── Global cleanup trap ────────────────────────────────────────────────────────
cleanup() {
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    rm -f /tmp/pubster_*.fifo /tmp/pubster_*.out
}
trap cleanup EXIT

# ── run_test ──────────────────────────────────────────────────────────────────
run_test() {
    local label="$1" fn="$2"
    if "$fn"; then
        echo "PASS: $label"
        ((PASS++)) || true
    else
        echo "FAIL: $label"
        ((FAIL++)) || true
    fi
}

# ── Build ─────────────────────────────────────────────────────────────────────
echo "--- Building ---"
cargo build --quiet 2>&1 | grep -v "^warning" || true

# ── Start server ──────────────────────────────────────────────────────────────
echo "--- Starting server ---"
"$SERVER" &
SERVER_PID=$!
sleep 1

# ── 1. basic_pubsub ───────────────────────────────────────────────────────────
# alice subscribes to rust; bob publishes "hello from bob"; alice receives it.
test_basic_pubsub() {
    local af="/tmp/pubster_alice.fifo" ao="/tmp/pubster_alice.out"
    rm -f "$af" "$ao"
    mkfifo "$af"

    "$CLIENT" <"$af" >"$ao" 2>&1 &
    local alice_pid=$!
    exec 3>"$af"            # hold write-fd open so FIFO never sees EOF

    echo "alice" >"$af"; sleep 0.3
    echo "1"     >"$af"     # subscribe
    echo "rust"  >"$af"; sleep 0.5

    printf 'bob\n2\nrust\nhello from bob\n4\n' | "$CLIENT" >/dev/null 2>&1
    sleep 0.5

    echo "4"  >"$af"
    exec 3>&-
    wait "$alice_pid" 2>/dev/null || true
    rm -f "$af"

    grep -q "hello from bob" "$ao"; local rc=$?
    rm -f "$ao"; return $rc
}

# ── 2. multiple_subscribers ───────────────────────────────────────────────────
# alice AND charlie both subscribe to rust; bob publishes "multicast";
# both must receive it.
test_multiple_subscribers() {
    local af="/tmp/pubster_alice.fifo"   ao="/tmp/pubster_alice.out"
    local cf="/tmp/pubster_charlie.fifo" co="/tmp/pubster_charlie.out"
    rm -f "$af" "$cf" "$ao" "$co"
    mkfifo "$af" "$cf"

    "$CLIENT" <"$af" >"$ao" 2>&1 &
    local alice_pid=$!
    "$CLIENT" <"$cf" >"$co" 2>&1 &
    local charlie_pid=$!
    exec 3>"$af"
    exec 4>"$cf"

    echo "alice"   >"$af"; echo "charlie" >"$cf"; sleep 0.3
    echo "1"       >"$af"; echo "rust"    >"$af"
    echo "1"       >"$cf"; echo "rust"    >"$cf"
    sleep 0.5

    printf 'bob\n2\nrust\nmulticast\n4\n' | "$CLIENT" >/dev/null 2>&1
    sleep 0.5

    echo "4" >"$af"; exec 3>&-
    echo "4" >"$cf"; exec 4>&-
    wait "$alice_pid" "$charlie_pid" 2>/dev/null || true
    rm -f "$af" "$cf"

    local rc=0
    grep -q "multicast" "$ao" || rc=1
    grep -q "multicast" "$co" || rc=1
    rm -f "$ao" "$co"; return $rc
}

# ── 3. topic_isolation ────────────────────────────────────────────────────────
# alice subscribes to rust, charlie to python; bob publishes to rust only;
# alice receives the message, charlie does NOT.
test_topic_isolation() {
    local af="/tmp/pubster_alice.fifo"   ao="/tmp/pubster_alice.out"
    local cf="/tmp/pubster_charlie.fifo" co="/tmp/pubster_charlie.out"
    rm -f "$af" "$cf" "$ao" "$co"
    mkfifo "$af" "$cf"

    "$CLIENT" <"$af" >"$ao" 2>&1 &
    local alice_pid=$!
    "$CLIENT" <"$cf" >"$co" 2>&1 &
    local charlie_pid=$!
    exec 3>"$af"
    exec 4>"$cf"

    echo "alice"   >"$af"; echo "charlie"  >"$cf"; sleep 0.3
    echo "1"       >"$af"; echo "rust"     >"$af"
    echo "1"       >"$cf"; echo "python"   >"$cf"
    sleep 0.5

    printf 'bob\n2\nrust\nisolation-test\n4\n' | "$CLIENT" >/dev/null 2>&1
    sleep 0.5

    echo "4" >"$af"; exec 3>&-
    echo "4" >"$cf"; exec 4>&-
    wait "$alice_pid" "$charlie_pid" 2>/dev/null || true
    rm -f "$af" "$cf"

    local rc=0
    grep -q  "isolation-test" "$ao" || rc=1   # alice must receive
    ! grep -q "isolation-test" "$co" || rc=1  # charlie must NOT receive
    rm -f "$ao" "$co"; return $rc
}

# ── 4. unsubscribe ────────────────────────────────────────────────────────────
# alice subscribes then unsubscribes; bob publishes "should not arrive";
# alice must NOT receive it.
test_unsubscribe() {
    local af="/tmp/pubster_alice.fifo" ao="/tmp/pubster_alice.out"
    rm -f "$af" "$ao"
    mkfifo "$af"

    "$CLIENT" <"$af" >"$ao" 2>&1 &
    local alice_pid=$!
    exec 3>"$af"

    echo "alice" >"$af"; sleep 0.2
    echo "1"     >"$af"     # subscribe
    echo "rust"  >"$af"; sleep 0.3
    echo "3"     >"$af"     # unsubscribe
    echo "rust"  >"$af"; sleep 0.3

    printf 'bob\n2\nrust\nshould not arrive\n4\n' | "$CLIENT" >/dev/null 2>&1
    sleep 0.3

    echo "4"  >"$af"
    exec 3>&-
    wait "$alice_pid" 2>/dev/null || true
    rm -f "$af"

    local rc=0
    grep -q "should not arrive" "$ao" && rc=1
    rm -f "$ao"; return $rc
}

# ── 5. self_publish ───────────────────────────────────────────────────────────
# alice subscribes to rust then publishes "echo test" to rust;
# alice receives her own message (no self-exclusion in broker).
test_self_publish() {
    local af="/tmp/pubster_alice.fifo" ao="/tmp/pubster_alice.out"
    rm -f "$af" "$ao"
    mkfifo "$af"

    "$CLIENT" <"$af" >"$ao" 2>&1 &
    local alice_pid=$!
    exec 3>"$af"

    echo "alice"     >"$af"; sleep 0.2
    echo "1"         >"$af"     # subscribe
    echo "rust"      >"$af"; sleep 0.3
    echo "2"         >"$af"     # publish
    echo "rust"      >"$af"
    echo "echo test" >"$af"; sleep 0.3

    echo "4"  >"$af"
    exec 3>&-
    wait "$alice_pid" 2>/dev/null || true
    rm -f "$af"

    grep -q "echo test" "$ao"; local rc=$?
    rm -f "$ao"; return $rc
}

# ── 6. list_topics ────────────────────────────────────────────────────────────
# alice subscribes to rust, charlie to python; alice calls list_topics (option 0);
# alice's output must contain both "rust" and "python".
test_list_topics() {
    local af="/tmp/pubster_alice.fifo"   ao="/tmp/pubster_alice.out"
    local cf="/tmp/pubster_charlie.fifo" co="/tmp/pubster_charlie.out"
    rm -f "$af" "$cf" "$ao" "$co"
    mkfifo "$af" "$cf"

    "$CLIENT" <"$af" >"$ao" 2>&1 &
    local alice_pid=$!
    "$CLIENT" <"$cf" >"$co" 2>&1 &
    local charlie_pid=$!
    exec 3>"$af"
    exec 4>"$cf"

    echo "alice"   >"$af"; echo "charlie"  >"$cf"; sleep 0.2
    echo "1"       >"$af"; echo "rust"     >"$af"
    echo "1"       >"$cf"; echo "python"   >"$cf"
    sleep 0.3

    echo "0"       >"$af"   # list topics
    sleep 0.2

    echo "4" >"$af"; exec 3>&-
    echo "4" >"$cf"; exec 4>&-
    wait "$alice_pid" "$charlie_pid" 2>/dev/null || true
    rm -f "$af" "$cf"

    local rc=0
    grep -q "rust"   "$ao" || rc=1
    grep -q "python" "$ao" || rc=1
    rm -f "$ao" "$co"; return $rc
}

# ── Run all tests ──────────────────────────────────────────────────────────────
run_test "basic_pubsub"         test_basic_pubsub
run_test "multiple_subscribers" test_multiple_subscribers
run_test "topic_isolation"      test_topic_isolation
run_test "unsubscribe"          test_unsubscribe
run_test "self_publish"         test_self_publish
run_test "list_topics"          test_list_topics

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
