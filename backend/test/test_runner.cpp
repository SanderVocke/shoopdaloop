#include <boost/ut.hpp>
#include "LoggingBackend.h"

using namespace boost::ut;

void usage(std::string name) {
    std::cout << "Usage: " << name << " [filter=\"FILTER\"] [-h | --help]" << std::endl;
}

int main() { }

// int main(int argc, const char *argv[]) {
//     std::string name(argv[0]);

//     logging::parse_conf_from_env();

//     for(size_t i = 1; i<(size_t) argc; i++) {
//         std::string arg(argv[i]);
//         auto filter_arg = std::string("filter=");
//         if (arg.substr(0, filter_arg.size()) == filter_arg) {
//             static auto filter = arg.substr(filter_arg.size());
//             std::cout << "Filtering on: \"" << filter << "\"" << std::endl;
            
//             cfg<override> = {.filter = filter};
//         } else if (arg.substr(0, 2) == "-h" || arg.substr(0, 6) == "--help") {
//             usage(std::string(argv[0]));
//             abort();
//         } else {
//             std::cerr << "Invalid argument." << std::endl;
//             usage(std::string(argv[0]));
//             abort();
//         }
//     }

//     return 0;
// }