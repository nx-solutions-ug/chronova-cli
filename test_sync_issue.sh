#!/bin/bash

# Test script to reproduce the sync offline activity issue

echo "=== Testing Chronova CLI Sync Offline Activity Issue ==="

# Build the CLI
echo "Building CLI..."
cargo build --release

# Create a test config file
echo "Creating test config..."
cat > test_config.cfg << EOF
[settings]
api_key = invalid-test-key
api_url = http://localhost:9999/invalid-endpoint
EOF

# Add some heartbeats to the queue with an invalid API URL to ensure they fail
echo "Adding test heartbeats to queue..."
./target/release/chronova-cli --config test_config.cfg --entity "/tmp/test1.rs" --plugin "test-plugin" --write

# Check queue status before sync
echo "Queue status before sync:"
./target/release/chronova-cli --config test_config.cfg --offline-count

# Try to sync offline activity
echo "Attempting to sync offline activity..."
./target/release/chronova-cli --config test_config.cfg --sync-offline-activity 5

# Check queue status after sync
echo "Queue status after sync:"
./target/release/chronova-cli --config test_config.cfg --offline-count

# Clean up
rm -f test_config.cfg

echo "=== Test completed ==="