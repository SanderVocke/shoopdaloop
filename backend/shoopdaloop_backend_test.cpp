#include "shoopdaloop_backend.h"

#include <iostream>
#include <math.h>
#include <string>
#include <unistd.h>
#include <vector>
#include <sstream>

using namespace std;

extern "C" {

void backend_abort_cb() {
    std::cerr << "Backend: aborted." << std::endl;
    exit(1);
}

int update_cb(
    unsigned n_loops,
    unsigned n_ports,
    loop_state_t *loop_states,
    int *loop_lengths,
    int *loop_positions,
    float *loop_volumes,
    float *port_volumes,
    float *port_passthrough_levels
) { std::cout << "Update!" << std::endl; return 0; }

}

void usage(char **argv) {
    std::cout << "Usage: " << argv[0] << " n_loops loop_len loops_per_port" << std::endl;
}

int main(int argc, char **argv) {
    if (argc != 4) {
        usage(argv);
        exit(1);
    }

    unsigned n_loops = stoi(std::string(argv[1]));
    unsigned loop_len = stoi(std::string(argv[2]));
    unsigned loops_per_port = stoi(std::string(argv[3]));
    size_t n_ports = (size_t) ceil((float) n_loops / (float) loops_per_port);
    std::vector<unsigned> loops_to_ports;
    std::vector<std::string> input_port_names, output_port_names;
    std::vector<const char*> _input_port_names, _output_port_names;
    for(size_t i=0; i<n_loops; i++) {
        loops_to_ports.push_back(i / loops_per_port);
        std::ostringstream iname, oname;
        iname << "in_" << i+1;
        oname << "out_" << i+1;
        input_port_names.push_back(iname.str());
        output_port_names.push_back(oname.str());
        _input_port_names.push_back(input_port_names[i].c_str());
        _output_port_names.push_back(output_port_names[i].c_str());
    }

    initialize(
        n_loops,
        n_ports,
        10.0f,
        loops_to_ports.data(),
        _input_port_names.data(),
        _output_port_names.data(),
        "ShoopDaLoop_backend_test",
        update_cb,
        backend_abort_cb
    );

    std::cout << "sleep" << std::endl;
    sleep(-1);
    std::cout << "done" << std::endl;
}
