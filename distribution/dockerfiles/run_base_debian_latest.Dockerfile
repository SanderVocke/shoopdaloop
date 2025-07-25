# syntax=docker/dockerfile:1
   
FROM debian:latest
WORKDIR /root
USER root

RUN DEBIAN_FRONTEND=noninteractive apt-get update
RUN DEBIAN_FRONTEND=noninteractive apt-get -y install $(dependencies/get_dependencies.sh run_base_debian_latest)