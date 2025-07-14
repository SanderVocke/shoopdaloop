#!/bin/bash

# This script scans a given input directory for ELF files containing debug information.
# For each such file, it extracts the debug info into a separate output directory,
# strips the debug info from the original file in-place, and creates a GNU debug link.
# It also reports on the process, including space reduction, debug info storage size, and total time taken.

# Function to display usage instructions for the script.
usage() {
    echo "Usage: $0 <input_folder> <output_folder>"
    echo "  <input_folder>: The source directory to scan recursively for ELF files."
    echo "  <output_folder>: The destination directory where extracted debug information files will be stored."
    echo "                   This folder will be created by the script. If it already exists,"
    echo "                   the script will exit with an error to prevent accidental overwrites."
    exit 1
}

# Function to convert byte counts into a human-readable format (B, KiB, MiB, GiB).
# This provides a more user-friendly output for file sizes.
human_readable_size() {
    local bytes=$1
    if (( bytes < 1024 )); then
        echo "${bytes} B"
    elif (( bytes < 1024 * 1024 )); then
        # Use 'bc' for floating-point division to get accurate KiB values.
        printf "%.2f KiB\n" "$(echo "scale=2; $bytes / 1024" | bc)"
    elif (( bytes < 1024 * 1024 * 1024 )); then
        # Use 'bc' for floating-point division to get accurate MiB values.
        printf "%.2f MiB\n" "$(echo "scale=2; $bytes / (1024 * 1024)" | bc)"
    else
        # Use 'bc' for floating-point division to get accurate GiB\n" "$(echo "scale=2; $bytes / (1024 * 1024 * 1024)" | bc)"
    fi
}

# --- Pre-flight Checks ---

# Verify that all necessary commands are available in the system's PATH.
# This ensures the script can execute its core functionalities.
for cmd in find file readelf objcopy stat basename dirname bc date; do # Added 'date' command check
    if ! command -v "$cmd" &> /dev/null; then
        echo "Error: Required command '$cmd' not found. Please install it to run this script."
        exit 1
    fi
done

# --- Argument Parsing and Validation ---

# Check if exactly two arguments (input_folder and output_folder) are provided.
if [ "$#" -ne 2 ]; then
    usage # If not, display usage and exit.
fi

input_dir="$1"
output_dir="$2"

# Validate the input directory: It must exist and be a directory.
if [ ! -d "$input_dir" ]; then
    echo "Error: Input directory '$input_dir' does not exist or is not a directory."
    exit 1
fi

# Validate the output directory: It must NOT exist to prevent accidental data loss.
if [ -e "$output_dir" ]; then
    echo "Error: Output directory '$output_dir' already exists. Please remove it or choose a different name."
    echo "       Exiting to prevent accidental data loss."
    exit 1
fi

# Attempt to create the output directory.
mkdir -p "$output_dir"
if [ $? -ne 0 ]; then
    echo "Error: Failed to create output directory '$output_dir'."
    exit 1
fi

echo "--- Starting ELF Debug Information Processing ---"
echo "Scanning input directory: '$input_dir'"
echo "Debug information will be saved to: '$output_dir'"
echo ""

# Record the start time in seconds since the epoch.
start_time=$(date +%s)

# --- Initialization of Counters ---

# These variables will track the progress and results of the processing.
processed_files_count=0      # Number of ELF files successfully processed.
total_original_size=0        # Sum of original sizes of all processed ELF files.
total_stripped_size=0        # Sum of sizes of all processed ELF files after stripping.
total_debug_info_storage=0   # Sum of sizes of all extracted debug information files.

# --- Main Processing Loop ---

# Use 'find' to recursively locate all files in the input directory.
# '-print0' and 'while IFS= read -r -d $'\0'' handle filenames with spaces or special characters safely.
find "$input_dir" -type f -print0 | while IFS= read -r -d $'\0' elf_file; do
    # 1. Check if the current file is an ELF file.
    # 'file -L' follows symlinks. 'grep -q 'ELF'' checks for the ELF identifier.
    if ! file -L "$elf_file" | grep -q 'ELF'; then
        continue # Not an ELF file, skip to the next file.
    fi

    # 2. Check if the ELF file contains common debug information sections.
    # 'readelf -S' lists section headers. '2>/dev/null' suppresses errors for non-ELF files.
    # We look for typical debug sections like .debug_info, .debug_abbrev, etc.
    if ! readelf -S "$elf_file" 2>/dev/null | grep -q '\.debug_info\|\.debug_abbrev\|\.debug_line\|\.debug_str\|\.debug_loc\|\.debug_aranges'; then
        continue # ELF file found, but no debug info, skip.
    fi

    echo "Processing file: $elf_file"

    # Get the original size of the ELF file before any modifications.
    current_original_size=$(stat -c%s "$elf_file")
    
    # Determine the base name of the file for the debug info file.
    base_name=$(basename "$elf_file")
    
    # 3. Handle naming conflicts in the output directory by adding a suffix.
    # Initialize the debug file path and a counter for suffixes.
    debug_file_path_base="$output_dir/$base_name"
    debug_file_path="${debug_file_path_base}.debug"
    suffix_counter=0

    # Loop until a unique filename is found for the debug information file.
    while [ -e "$debug_file_path" ]; do
        suffix_counter=$((suffix_counter + 1))
        # Construct the new filename with a numerical suffix before the .debug extension.
        debug_file_path="${debug_file_path_base}_${suffix_counter}.debug"
        echo "  Warning: Naming conflict detected for '${base_name}.debug'. Trying alternative name: '$(basename "$debug_file_path")'"
    done

    # 4. Check if the ELF file has a GNU Build ID.
    # 'readelf -n' displays notes. 'grep -q 'GNU_BUILD_ID'' checks for the presence of the build ID.
    # This is for informational purposes only, and does not stop processing.
    if ! readelf -n "$elf_file" 2>/dev/null | grep -q 'GNU_BUILD_ID'; then
        echo "  Warning: ELF file '$elf_file' does not have a GNU Build ID."
    fi

    # 5. Extract debug information to the separate debug file.
    echo "  Extracting debug info to: '$debug_file_path'"
    # '--only-debug': Extracts only debug sections.
    # '--compress-debug-sections': Compresses the debug sections in the output file.
    objcopy --only-debug --compress-debug-sections "$elf_file" "$debug_file_path"
    if [ $? -ne 0 ]; then
        echo "  Error: Failed to extract debug info from '$elf_file'."
        echo "         Skipping this file and cleaning up partial debug file."
        rm -f "$debug_file_path" # Clean up partially created debug file.
        echo ""
        continue
    fi
    # Get the size of the newly created debug information file.
    current_debug_info_size=$(stat -c%s "$debug_file_path")
    total_debug_info_storage=$((total_debug_info_storage + current_debug_info_size))

    # 6. Strip debug information from the original ELF file (in-place).
    echo "  Stripping debug info from original file: '$elf_file'"
    # '--strip-debug': Removes all debug sections from the file.
    objcopy --strip-debug "$elf_file"
    if [ $? -ne 0 ]; then
        echo "  Error: Failed to strip debug info from '$elf_file'."
        echo "         Removing extracted debug file and skipping this file."
        rm -f "$debug_file_path" # Clean up the extracted debug file.
        echo ""
        continue
    fi
    # Get the size of the original file after stripping.
    current_stripped_size=$(stat -c%s "$elf_file")

    # 7. Create a GNU debug link in the original ELF file.
    echo "  Adding GNU debug link to: '$elf_file'"
    # '--add-gnu-debuglink': Adds a .gnu_debuglink section pointing to the separate debug file.
    # Use basename of the *chosen* debug_file_path for the debug link.
    objcopy --add-gnu-debuglink="$(basename "$debug_file_path")" "$elf_file"
    if [ $? -ne 0 ]; then
        echo "  Error: Failed to add GNU debug link to '$elf_file'."
        echo "         Removing extracted debug file and skipping this file."
        rm -f "$debug_file_path" # Clean up the extracted debug file.
        echo ""
        continue
    fi

    # Update global counters for successful processing.
    processed_files_count=$((processed_files_count + 1))
    total_original_size=$((total_original_size + current_original_size))
    total_stripped_size=$((total_stripped_size + current_stripped_size))
    echo "" # Add a newline for better readability between processed files.
done

# Record the end time in seconds since the epoch.
end_time=$(date +%s)

# Calculate the total elapsed time.
total_time_seconds=$((end_time - start_time))

# --- Final Report ---

echo "--- Processing Complete ---"
echo "Summary of operations:"
echo "  Amount of ELF files processed: $processed_files_count"

# Calculate the total storage space reduced.
# This is the difference between the sum of original sizes and the sum of stripped sizes.
space_reduced=$((total_original_size - total_stripped_size))
echo "  Total storage space reduced in the source folder: $(human_readable_size "$space_reduced")"
echo "  Total size of the debug information folder: $(human_readable_size "$total_debug_info_storage")"
echo "  Total time taken: ${total_time_seconds} seconds"

exit 0
