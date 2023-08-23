#!/usr/bin/env sh

exec docker run -v "/var/run/docker.sock":"/var/run/docker.sock" sandervocke/qoverage:latest $INPUT_COMMAND