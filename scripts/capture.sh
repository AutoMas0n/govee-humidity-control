#!/bin/bash
# BLE H5080 Protocol Capture Helper
# Usage:
#   ./scripts/capture.sh start    # Start btmon capture
#   ./scripts/capture.sh stop     # Stop btmon and save
#   ./scripts/capture.sh replay   # Replay captured commands
#   ./scripts/capture.sh parse    # Parse capture to text

CAPTURE_FILE="/tmp/h5080_sniff.hci"
TEXT_FILE="/tmp/h5080_capture.txt"
PLUG_MAC="${1:-D4:AD:FC:41:E1:DD}"

case "${1:-}" in
  start)
    echo "Starting btmon capture to $CAPTURE_FILE"
    echo "Now toggle the plug via Govee app!"
    sudo btmon -w "$CAPTURE_FILE" &
    echo "PID: $!"
    echo "Run: $0 stop"
    ;;
  stop)
    echo "Stopping capture..."
    sudo pkill -INT btmon 2>/dev/null || sudo pkill btmon 2>/dev/null
    sleep 1
    ls -la "$CAPTURE_FILE"
    echo "Convert to text: $0 parse"
    ;;
  parse)
    echo "Parsing $CAPTURE_FILE -> $TEXT_FILE"
    sudo btmon -r "$CAPTURE_FILE" > "$TEXT_FILE"
    wc -l "$TEXT_FILE"
    echo "--- ATT Write commands ---"
    grep -E "ATT Write|Write Command|Write Request" "$TEXT_FILE" || echo "(none found)"
    ;;
  replay)
    shift
    PAYLOAD="$1"
    HANDLE="$2"
    if [ -z "$PAYLOAD" ] || [ -z "$HANDLE" ]; then
      echo "Usage: $0 replay <hex_payload> <handle>"
      echo "Example: $0 replay 3301010000000000000000000000000000000033 0x0011"
      exit 1
    fi
    echo "Writing $PAYLOAD to handle $HANDLE on $PLUG_MAC..."
    sudo timeout 15 gatttool -b "$PLUG_MAC" --char-write -a "$HANDLE" -n "$PAYLOAD"
    echo "Done. Did it click?"
    ;;
  *)
    echo "Usage:"
    echo "  $0 start              # Start btmon capture"
    echo "  $0 stop               # Stop capture"
    echo "  $0 parse              # Parse capture to readable text"
    echo "  $0 replay <hex> <h>   # Replay a command to a handle"
    ;;
esac