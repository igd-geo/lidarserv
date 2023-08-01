import json

def merge_json_files(slice_count, output_file):
    merged_data = {}

    for slice_number in range(1, slice_count + 1):
        file_name = f"output_slices_100000p/freiburg_slice{slice_number}_2023-07-31_1.json"
        try:
            with open(file_name, 'r') as file:
                data = json.load(file)
                merged_data[slice_number] = data
        except FileNotFoundError:
            print(f"Warning: File '{file_name}' not found. Skipping...")

    with open(output_file, 'w') as output:
        json.dump(merged_data, output, indent=4)

if __name__ == "__main__":
    slice_count = 20
    output_file = "merged_output/merged_freiburg_100000p.json"
    merge_json_files(slice_count, output_file)
    print("JSON files merged successfully.")
