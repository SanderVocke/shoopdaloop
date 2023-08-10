#include <boost/ut.hpp>
#include "LoggingBackend.h"

using namespace boost::ut;

int main() {
    logging::parse_conf_from_env();
}