# syntax=docker/dockerfile:1
   
FROM archlinux:base-devel
WORKDIR /root
USER root

RUN pacman --noconfirm -Syy && \
    pacman --noconfirm -Syu
RUN pacman --noconfirm -S --needed --overwrite "*" $(dependencies/get_dependencies.sh run_base_arch)