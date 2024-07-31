# syntax=docker/dockerfile:1
   
FROM debian:bullseye
WORKDIR /

COPY dependencies dependencies

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install software-properties-common && \
    DEBIAN_FRONTEND=noninteractive apt-add-repository -y "deb http://deb.debian.org/debian bullseye-backports main contrib non-free" && \
    DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install $(dependencies/get_dependencies.sh build_base_debian_bullseye)

RUN gem install fpm

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -c 'sh /dev/stdin "$@"' _ -v -y && \
    ln -s /root/.cargo/bin/cargo /usr/local/bin/cargo && \
    ln -s /root/.cargo/bin/rustc /usr/local/bin/rustc && \
    ln -s /root/.cargo/bin/rustup /usr/local/bin/rustup