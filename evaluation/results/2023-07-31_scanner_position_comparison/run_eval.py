import subprocess

def run_command_frankfurt(slice_number):
    command = f"cargo run --release --bin evaluation tomls/frankfurt_slice{slice_number}.toml"
    subprocess.run(command, shell=True)

def run_command_freiburg(slice_number):
    command = f"cargo run --release --bin evaluation tomls/freiburg_slice{slice_number}.toml"
    subprocess.run(command, shell=True)

def main():
    subprocess.run("python generate_toml.py", shell=True)

    for slice_num in range(1, 21):
        run_command_frankfurt(slice_num)
    for slice_num in range(1, 21):
        run_command_freiburg(slice_num)

if __name__ == "__main__":
    main()
