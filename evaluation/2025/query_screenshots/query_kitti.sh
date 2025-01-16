#!/bin/bash

OUTPUT_DIR="kitti"
mkdir -p "$OUTPUT_DIR"

QUERIES=(
  "attr(semantic <= 12)"
  "attr(semantic == 11)"
  "attr(35 <= PointSourceID <= 64)"
  "attr(208 <= PointSourceID <= 248)"
  "attr(199083995.09382153 <= GpsTime <= 466372692.21052635)"
  "attr(687577131.20366132 <= GpsTime <= 805552832.00000000)"
  "attr(visible <= 1)"
  "attr(ColorRGB <= [10,10,10])"
)

for QUERY in "${QUERIES[@]}"; do
  filename="${OUTPUT_DIR}/query_${QUERY// /_}_pointwise.las"
  cargo run --release --bin lidarserv-query -- --outfile "$filename" "$QUERY"
  filename="${OUTPUT_DIR}/query_${QUERY// /_}_nodewise.las"
  cargo run --release --bin lidarserv-query -- --outfile "$filename" --disable-point-filtering "$QUERY"
done
