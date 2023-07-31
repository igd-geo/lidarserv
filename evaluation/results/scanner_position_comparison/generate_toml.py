for i in range(1, 21):
    with open('scanner_position_comparison_freiburg.toml', 'r') as file:
        content = file.read()

    # Replace "slice1" with an incrementing number
    new_content = content.replace('slice1', f'slice{i}')

    # Write the new content to a new file
    with open(f'tomls/freiburg_slice{i}.toml', 'w') as new_file:
        new_file.write(new_content)

for i in range(1, 21):
    with open('scanner_position_comparison_frankfurt.toml', 'r') as file:
        content = file.read()

    # Replace "slice1" with an incrementing number
    new_content = content.replace('slice1', f'slice{i}')

    # Write the new content to a new file
    with open(f'tomls/frankfurt_slice{i}.toml', 'w') as new_file:
        new_file.write(new_content)
