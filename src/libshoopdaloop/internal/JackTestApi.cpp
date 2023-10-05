#include "JackTestApi.h"
#include "LoggingBackend.h"

std::map<std::string, JackTestApi::Client> JackTestApi::ms_clients;
logging::logger* JackTestApi::ms_logger;