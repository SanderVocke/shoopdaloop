# syntax=docker/dockerfile:1
   
FROM debian:bullseye
WORKDIR /

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install sudo && \
    useradd -m build && \
    echo "build ALL=(ALL) NOPASSWD :ALL" > /etc/sudoers.d/10-build

USER build
WORKDIR /home/build
COPY dependencies dependencies

RUN  sudo DEBIAN_FRONTEND=noninteractive apt-get -y install software-properties-common && \
     sudo DEBIAN_FRONTEND=noninteractive apt-add-repository -y "deb http://deb.debian.org/debian bullseye-backports main contrib non-free" && \
     sudo DEBIAN_FRONTEND=noninteractive apt-get update && \
     sudo DEBIAN_FRONTEND=noninteractive apt-get -y install $(dependencies/get_dependencies.sh build_base_debian_bullseye)