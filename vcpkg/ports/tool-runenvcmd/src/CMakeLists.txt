cmake_minimum_required(VERSION 3.10)
project(RunEnvCmd C)

# Set C standard
set(CMAKE_C_STANDARD 99)

# Add the executable
add_executable(run_env_cmd run_env_cmd.c)

install(TARGETS run_env_cmd
        RUNTIME DESTINATION tools/run_env_cmd)