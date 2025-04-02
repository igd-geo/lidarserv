base_path="../../../../data/"
declare -a arr=("Lille_sorted.las" "kitti_sorted.las" "AHN4.las")

for i in "${arr[@]}"
do
  echo "File $i"
  cargo run --release --bin insertion -- --input-file $base_path$i --compression none
  cargo run --release --bin query -- --input-file $base_path$i
  cargo run --release --bin insertion -- --input-file $base_path$i --compression dimensional
  cargo run --release --bin query -- --input-file $base_path$i
done
