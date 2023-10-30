# syntax=docker/dockerfile:1
   
FROM debian:bookworm
WORKDIR /

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install sudo && \
    useradd -m build && \
    echo "build ALL=(ALL) NOPASSWD :ALL" > /etc/sudoers.d/10-build

USER build
WORKDIR /home/build
COPY dependencies dependencies

RUN sudo DEBIAN_FRONTEND=noninteractive apt-get update -y && \
    sudo DEBIAN_FRONTEND=noninteractive apt-get -y install software-properties-common && \
    sudo add-apt-repository "deb https://deb.debian.org/debian experimental main" && \
    sudo DEBIAN_FRONTEND=noninteractive apt-get update -y
RUN sudo DEBIAN_FRONTEND=noninteractive apt-get -y install $(dependencies/get_dependencies.sh build_base)
RUN sudo DEBIAN_FRONTEND=noninteractive apt-get -y -t experimental install $(dependencies/get_dependencies.sh build_base_experimental)