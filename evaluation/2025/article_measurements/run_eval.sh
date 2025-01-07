#!/usr/bin/env bash

data="../../../data"
potree_converter=/home/localadmin/PotreeConverter/build/PotreeConverter
PG="../pgPointCloud_Measurements/"

# set -o xtrace

 lidarserv measurements
 echo "RUNNING LIDARSERV MEASUREMENTS"
 for input in lille kitti ahn4; do
 #for input in lille; do
  echo Measuring $input
  lidarserv-evaluation $input.toml > lidarserv_$input.out 2>&1
  rm -rf $data/${input}-eval
 done

# potree measurements
# echo "RUNNING POTREE MEASUREMENTS"
# for input in Lille_sorted.las kitti_sorted.las AHN4.las; do
# #for input in Lille_sorted.las; do
#  echo Measuring $input
# 
#  $potree_converter $data/$input -o $data/${input}_Potree -m poisson --encoding UNCOMPRESSED > ${input}_uncompressed.out
#  du -s $data/${input}_Potree >> ${input}_uncompressed.out
#  rm -rf $data/${input}_Potree
# 
#  $potree_converter $data/$input -o $data/${input}_Potree -m poisson --encoding BROTLI > ${input}_compressed.out
#  du -s $data/${input}_Potree >> ${input}_compressed.out
#  rm -rf $data/${input}_Potree
# done

# pgpointcloud measurements
#echo "RUNNING PGPOINTCLOUD MEASUREMENTS"
#cd ../pgPointCloud_Measurements/
##for input in Lille_sorted.las kitti_sorted.las AHN4.las; do
#for input in Lille_sorted.las; do
# echo Measuring $input
# cargo run --release --bin insertion -- --input-file $data/$input --compression none > ${input}_pdal_insertion_uncompressed.out 2>&1
# cargo run --release --bin query -- --input-file $data/$input --drop-table > ${input}_pdal_query_uncompressed.out 2>&1
#
# cargo run --release --bin insertion -- --input-file $data/$input --compression BROTLI > ${input}_pdal_insertion_brotli.out 2>&1
# cargo run --release --bin query -- --input-file $data/$input --drop-table > ${input}_pdal_query_brotli.out 2>&1
#done

