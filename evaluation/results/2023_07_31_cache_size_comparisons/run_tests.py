import subprocess
#python scripts are cooler than bash scripts

def main():
    command = f"cargo run --release --bin evaluation cache_size_comparison_frankfurt.toml"
    subprocess.run(command, shell=True)
    command = f"cargo run --release --bin evaluation cache_size_comparison_freiburg.toml"
    subprocess.run(command, shell=True)

if __name__ == "__main__":
    main()