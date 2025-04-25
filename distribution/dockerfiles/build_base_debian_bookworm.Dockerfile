# syntax=docker/dockerfile:1
   
FROM debian:bookworm
WORKDIR /

COPY dependencies dependencies

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install software-properties-common && \
    DEBIAN_FRONTEND=noninteractive apt-add-repository -y "deb http://deb.debian.org/debian main contrib non-free" && \
    DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install $(dependencies/get_dependencies.sh build_base_debian_bookworm)

RUN gem install fpm

RUN curl https://pyenv.run | bash

ENV PATH="$PATH:/root/.pyenv/bin"