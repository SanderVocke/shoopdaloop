#include <boost/ut.hpp>
#include "LoggingBackend.h"

using namespace boost::ut;

void usage(std::string name) {
    std::cout << "Usage: " << name << " [filter=\"FILTER\"] [-h | --help]" << std::endl;
}

int main() {
    logging::parse_conf_from_env();
}