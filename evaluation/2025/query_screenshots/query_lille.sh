#!/bin/bash

OUTPUT_DIR="lille"
mkdir -p "$OUTPUT_DIR"

QUERIES=(
  "attr(Intensity > 128)"
  "attr(Intensity <= 2)"
  "attr(GpsTime < 20000)"
  "attr(GpsTime > 23000)"
  "attr(PointSourceID >= 10)"
  "attr(PointSourceID >= 5)"
  "attr(ScanAngleRank <= 45)"
  "attr(ScanAngleRank <= 90)"
  "view_frustum( camera_pos: [-560.45, -584.87, 47.29], camera_dir: [0.75, 0.65, -0.12], camera_up: [0.0, 0.0, 1.0], fov_y: 0.78, z_near: 3.9, z_far: 3994169.6, window_size: [500.0, 500.0], max_distance: 10.0 )"
)

for QUERY in "${QUERIES[@]}"; do
  filename="${OUTPUT_DIR}/query_${QUERY// /_}_pointwise.las"
  cargo run --release --bin lidarserv-query -- --outfile "$filename" "$QUERY"
  filename="${OUTPUT_DIR}/query_${QUERY// /_}_nodewise.las"
  cargo run --release --bin lidarserv-query -- --outfile "$filename" --disable-point-filtering "$QUERY"
done
