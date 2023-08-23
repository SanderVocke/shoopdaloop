#!/usr/bin/env bash

exec docker run -v "/var/run/docker.sock":"/var/run/docker.sock" sandervocke/qoverage:latest $INPUT_COMMAND