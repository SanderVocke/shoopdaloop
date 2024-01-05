#include "shoop_crashhandling.h"
#include <filesystem>
#include <iostream>

int main() {
    auto dir = std::filesystem::temp_directory_path().string();
    std::cout << "Setting up with dir = " << dir << std::endl;
    shoop_init_crashhandling(dir.c_str());
    shoop_test_crash_segfault();
}