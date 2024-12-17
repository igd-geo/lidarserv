#!/usr/bin/env bash

DATA="../../../data"

lidarserv-evaluation lille.toml > lidarserv_lille.out
rm -rf $DATA/lille-eval
lidarserv-evaluation kitti.toml > lidarserv_kitti.out
rm -rf $DATA/kitti-eval
lidarserv-evaluation ahn4.toml > lidarserv_ahn4.out
rm -rf $DATA/ahn4-eval
PotreeConverter $DATA/Lille_sorted.las $DATA/Lille_Potree -m poisson --encoding UNCOMPRESSED > lille_uncompressed.out
du -s $DATA/Lille_sorted.las_converted >> lille_uncompressed.out
rm -rf $DATA/Lille_sorted.las_converted
PotreeConverter $DATA/Lille_sorted.las $DATA/Lille_Potree -m poisson --encoding BROTLI > lille_compressed.out
du -s $DATA/Lille_sorted.las_converted >> lille_compressed.out
rm -rf $DATA/Lille_sorted.las_converted
PotreeConverter $DATA/kitti_sorted.las $DATA/Kitti_Potree -m poisson --encoding UNCOMPRESSED > kitti_uncompressed.out
du -s $DATA/Kitti_sorted.las_converted >> kitti_uncompressed.out
rm -rf $DATA/kitti_sorted.las_converted
PotreeConverter $DATA/kitti_sorted.las $DATA/Kitti_Potree -m poisson --encoding BROTLI > kitti_compressed.out
du -s $DATA/Kitti_sorted.las_converted >> kitti_compressed.out
rm -rf $DATA/kitti_sorted.las_converted
PotreeConverter $DATA/AHN4.las $DATA/AHN4_Potree -m poisson --encoding UNCOMPRESSED > ahn_uncompressed.out
du -s $DATA/AHN4.las_converted >> ahn_uncompressed.out
rm -rf $DATA/AHN4.las_converted
PotreeConverter $DATA/AHN4.las data/AHN4_Potree -m poisson --encoding BROTLI > ahn_compressed.out
du -s $DATA/AHN4_las_converted >> ahn_compressed.out
rm -rf $DATA/AHN4.las_converted

