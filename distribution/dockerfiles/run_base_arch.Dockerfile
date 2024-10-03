# syntax=docker/dockerfile:1
   
FROM archlinux:base-devel
WORKDIR /

RUN pacman --noconfirm -Syy && \
    pacman --noconfirm -Syu && \
    pacman --noconfirm -S sudo && \
    useradd -m build && \
    echo "build ALL=(ALL) NOPASSWD :ALL" > /etc/sudoers.d/10-build

USER build
WORKDIR /home/build
COPY dependencies dependencies

RUN sudo pacman --noconfirm -S --needed --overwrite "*" $(dependencies/get_dependencies.sh run_base_arch)