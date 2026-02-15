#!/bin/bash

# Add some heartbeats that will fail to sync
echo "Adding heartbeats that will fail to sync..."
./target/debug/chronova-cli --entity "/tmp/test1.rs" --api-url "http://invalid-url.local" --write &
./target/debug/chronova-cli --entity "/tmp/test2.rs" --api-url "http://invalid-url.local" --write &
./target/debug/chronova-cli --entity "/tmp/test3.rs" --api-url "http://invalid-url.local" --write &

# Wait for the commands to finish
wait

echo "Checking queue status..."
./target/debug/chronova-cli --offline-count

echo "Running sync command..."
./target/debug/chronova-cli --sync-offline-activity 5

echo "Checking queue status after sync..."
./target/debug/chronova-cli --offline-count
