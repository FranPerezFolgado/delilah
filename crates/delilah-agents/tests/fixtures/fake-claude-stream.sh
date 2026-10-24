#!/bin/bash
# A stand-in for `claude -p --input-format stream-json --output-format stream-json`, with the
# wire shapes recorded in docs/spikes/2026-10-24-claude-stream.md. Tests only.
#   - like the real CLI it prints `system` hook lines at spawn and the `system` init line once
#     per turn, AFTER it has read the turn's user line (nothing but hooks before that);
#   - a user turn prints message_start, two text deltas, the assistant message, a
#     rate_limit_event and a result;
#   - a line containing WAIT starts the turn (message_start + one delta) and then hangs until
#     an `interrupt` control request arrives;
#   - a line containing NEEDS_PERMISSION asks `can_use_tool` and waits for the control_response;
#     (EDIT_A / EDIT_B ask for an Edit of src/a.rs / src/b.rs, FETCH_A / FETCH_B for a
#     WebFetch of docs.rs / example.com; ECHO_DENY makes a denial's message show in the result);
#   - a line containing UNKNOWN_EVENT prints a line of an unknown type before the turn;
#   - a line containing STDERR_NOISE writes to stderr;
#   - a line containing LIMIT runs a turn that hits the usage limit: a rate_limit_event with a
#     rejected status, then an error result;
#   - a line containing EXIT makes the process exit with code 3.
n=0
echo '{"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup"}'
echo '{"type":"system","subtype":"hook_response","hook_name":"SessionStart:startup","outcome":"success"}'

init() {
  echo '{"type":"system","subtype":"init","session_id":"sess-fake","cwd":"/tmp"}'
}

finish_turn() {
  local id="$1" text="$2"
  printf '{"type":"assistant","message":{"id":"%s","role":"assistant","content":[{"type":"text","text":"%s"}]}}\n' "$id" "$text"
  echo '{"type":"stream_event","event":{"type":"content_block_stop","index":0}}'
  echo '{"type":"stream_event","event":{"type":"message_stop"}}'
  echo '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"five_hour":{"resetsAt":1792872000,"utilization":0.41},"seven_day":{"resetsAt":1793113200,"utilization":0.29}}}}'
  printf '{"type":"result","subtype":"success","is_error":false,"result":"%s"}\n' "$text"
}

start_message() {
  printf '{"type":"stream_event","event":{"type":"message_start","message":{"id":"%s","role":"assistant","content":[]}}}\n' "$1"
  echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}'
}

delta() {
  printf '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"%s"}}}\n' "$1"
}

interrupt() {
  local rid="$1"
  printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{"still_queued":[]}}}\n' "$rid"
  echo '{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}'
  echo '{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["[ede_diagnostic] result_type=user"]}'
}

while IFS= read -r line; do
  case "$line" in
    *'"interrupt"'*)
      interrupt "$(printf '%s' "$line" | sed -E 's/.*"request_id":"([^"]*)".*/\1/')"
      continue ;;
    *'"control_response"'*) continue ;;
  esac
  n=$((n + 1))
  id="msg_$n"
  [[ "$line" == *EXIT* ]] && exit 3
  init
  [[ "$line" == *STDERR_NOISE* ]] && echo "fake-claude stderr noise" >&2
  [[ "$line" == *UNKNOWN_EVENT* ]] && echo '{"type":"mystery","payload":1}'
  start_message "$id"
  delta "Hel"
  if [[ "$line" == *WAIT* ]]; then
    continue
  fi
  if [[ "$line" == *NEEDS_PERMISSION* ]]; then
    tool=Bash input='{"command":"npm --version"}'
    [[ "$line" == *EDIT_A* ]] && tool=Edit input='{"file_path":"src/a.rs"}'
    [[ "$line" == *EDIT_B* ]] && tool=Edit input='{"file_path":"src/b.rs"}'
    [[ "$line" == *FETCH_A* ]] && tool=WebFetch input='{"url":"https://docs.rs/serde"}'
    [[ "$line" == *FETCH_B* ]] && tool=WebFetch input='{"url":"https://example.com/x"}'
    printf '{"type":"control_request","request_id":"perm-1","request":{"subtype":"can_use_tool","tool_name":"%s","tool_use_id":"toolu_1","input":%s,"permission_suggestions":[{"behavior":"allow","type":"addRules"}]}}\n' "$tool" "$input"
    while IFS= read -r reply; do
      [[ "$reply" == *'"control_response"'* ]] && break
    done
    if [[ "$reply" == *'"behavior":"allow"'* ]]; then
      echo '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"10.9.9","is_error":false}]}}'
      finish_turn "$id" "allowed"
    else
      echo '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"denied","is_error":true}]}}'
      if [[ "$line" == *ECHO_DENY* ]]; then
        finish_turn "$id" "denied: $(printf '%s' "$reply" | sed -E 's/.*"message":"([^"]*)".*/\1/')"
      else
        finish_turn "$id" "denied"
      fi
    fi
    continue
  fi
  if [[ "$line" == *LIMIT* ]]; then
    echo '{"type":"stream_event","event":{"type":"message_stop"}}'
    echo '{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour","resetsAt":1792872000,"unifiedWindows":{"five_hour":{"resetsAt":1792872000,"utilization":1.0}}}}'
    echo '{"type":"result","subtype":"success","is_error":true,"result":"You have hit your usage limit"}'
    continue
  fi
  delta "lo"
  finish_turn "$id" "Hello"
done
