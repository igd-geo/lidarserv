#!/bin/bash

DATA_DIR="../../../data/lille"
#QUERY="full"
QUERY="attr(Intensity > 128)"

# Function to clean up background processes
cleanup() {
  echo "Cleaning up background processes..."
  pkill lidarserv
}

# Trap the termination signals (SIGINT and SIGTERM) to run the cleanup function
trap cleanup SIGINT SIGTERM

# Remove the contents of the directory if it exists
if [ -d "$DATA_DIR" ]; then
  echo "Removing contents of $DATA_DIR..."
  find "$DATA_DIR" -mindepth 1 -delete
fi

# Create the directory
echo "Creating directory $DATA_DIR..."
mkdir -p "$DATA_DIR"

# Copy the settings file
echo "Copying settings file to $DATA_DIR..."
cp lille.json "$DATA_DIR/settings.json"

# Run the necessary commands
echo "Starting lidarserv-server..."
nohup cargo run --release --bin lidarserv-server -- serve "$DATA_DIR" > server.log 2>&1 &
sleep 3

echo "Starting lidarserv-input-file..."
nohup cargo run --release --bin lidarserv-input-file -- replay ../../../data/Lille_sorted.las --autoskip > input_file.log 2>&1 &
sleep 5

echo "Starting lidarserv-viewer..."
nohup cargo run --release --bin lidarserv-viewer -- --query "$QUERY" --point-color intensity --point-size 5 --point-distance 5 > viewer.log 2>&1 &

# Wait for all background processes to complete
echo "Waiting for all background processes to complete..."
wait

echo "All processes completed."