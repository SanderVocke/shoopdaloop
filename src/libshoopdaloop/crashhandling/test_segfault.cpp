#include "shoop_crashhandling.h"
#include <filesystem>
#include <iostream>

void callback(const char* dumped_file) {
    std::cout << "Callback triggered with " << dumped_file << std::endl;
}

int main() {
    auto dir = std::filesystem::temp_directory_path().string();
    std::cout << "Setting up with dir = " << dir << std::endl;
    shoop_init_crashhandling(dir.c_str(), callback);
    shoop_test_crash_segfault();
}