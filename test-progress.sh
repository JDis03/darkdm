#!/bin/bash
# Test progress bar with a slow download simulation

echo "Testing DarkDM ILoveCandy progress bar..."
echo ""

# Test with httpbin (small file, instant)
./src-tauri/target/release/darkdm descargar "https://httpbin.org/bytes/10240" --output /tmp/darkdm-test

echo ""
echo "✓ Test complete. Check /tmp/darkdm-test/ for downloaded file."
