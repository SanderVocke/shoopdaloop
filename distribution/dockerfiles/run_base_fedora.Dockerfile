# syntax=docker/dockerfile:1
   
FROM fedora:latest
WORKDIR /root
USER root
COPY dependencies dependencies

RUN dnf -y install $(dependencies/get_dependencies.sh run_base_fedora)