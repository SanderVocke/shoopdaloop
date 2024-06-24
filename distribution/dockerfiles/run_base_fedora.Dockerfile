# syntax=docker/dockerfile:1
   
FROM fedora:latest
WORKDIR /

RUN dnf -y install sudo && \
    useradd -m build && \
    echo "build ALL=(ALL) NOPASSWD :ALL" > /etc/sudoers.d/10-build

USER build
WORKDIR /home/build
COPY dependencies dependencies

RUN sudo dnf -y install $(dependencies/get_dependencies.sh run_base_fedora)