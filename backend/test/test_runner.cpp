#include <boost/ut.hpp>

using namespace boost::ut;

void usage(std::string name) {
    std::cout << "Usage: " << name << " [filter=FILTER]" << std::endl;
}

int main(int argc, const char *argv[]) {
    std::string name(argv[0]);

    for(size_t i = 1; i<(size_t) argc; i++) {
        std::string arg(argv[i]);
        auto filter_arg = std::string("filter=");
        if (arg.substr(0, filter_arg.size()) == filter_arg) {
            static auto filter = arg.substr(filter_arg.size());
            std::cout << "Filtering on: \"" << filter << "\"" << std::endl;
            
            cfg<override> = {.filter = filter};
        }
    }
}