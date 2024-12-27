#!/usr/bin/env bash

data="../../../data"
POTREE="../../../../PotreeConverter/build"
PG="../pgPointCloud_Measurements/"

# lidarserv measurements
echo "RUNNING LIDARSERV MEASUREMENTS"
for input in lille kitti ahn4; do
#for input in lille; do
 echo Measuring $input
 lidarserv-evaluation $input.toml > lidarserv_$input.out 2>&1
 rm -rf $DATA/$input-eval
done

# potree measurements
echo "RUNNING POTREE MEASUREMENTS"
for input in Lille_sorted.las kitti_sorted.las AHN4.las; do
#for input in Lille_sorted.las; do
 echo Measuring $input
 $POTREE/PotreeConverter $DATA/$input $DATA/${input}_Potree -m poisson --encoding UNCOMPRESSED > ${input}_uncompressed.out
 du -s $DATA/${input}_converted >> ${input}_uncompressed.out
 rm -rf $DATA/${input}_converted
 $POTREE/PotreeConverter $DATA/$input $DATA/${input}_Potree -m poisson --encoding BROTLI > ${input}_compressed.out
 du -s $DATA/${input}_converted >> ${input}_compressed.out
 rm -rf $DATA/${input}_converted
done

# pgpointcloud measurements
echo "RUNNING PGPOINTCLOUD MEASUREMENTS"
cd ../pgPointCloud_Measurements/
for input in Lille_sorted.las kitti_sorted.las AHN4.las; do
#for input in Lille_sorted.las; do
 echo Measuring $input
 cargo run --release --bin insertion -- --input-file $DATA/$input --compression none
 cargo run --release --bin query -- --input-file $DATA/$input --drop-table
done

