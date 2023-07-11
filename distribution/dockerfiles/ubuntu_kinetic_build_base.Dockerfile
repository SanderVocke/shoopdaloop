# syntax=docker/dockerfile:1
   
FROM ubuntu:kinetic
WORKDIR /

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install sudo && \
    useradd -m build && \
    echo "build ALL=(ALL) NOPASSWD :ALL" > /etc/sudoers.d/10-build

USER build
WORKDIR /home/build
COPY dependencies dependencies

RUN sudo DEBIAN_FRONTEND=noninteractive apt-get -y install $(dependencies/get_dependencies.sh ubuntu_kinetic_build_base)