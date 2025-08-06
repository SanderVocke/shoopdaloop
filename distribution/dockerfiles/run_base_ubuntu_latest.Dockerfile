# syntax=docker/dockerfile:1
   
FROM ubuntu:latest
WORKDIR /root
USER root

COPY dependencies dependencies

RUN DEBIAN_FRONTEND=noninteractive apt-get update
RUN DEBIAN_FRONTEND=noninteractive apt-get -y install $(dependencies/get_dependencies.sh run_base_debian_latest)