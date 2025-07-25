# syntax=docker/dockerfile:1
   
FROM debian:bookworm
WORKDIR /root
USER root

COPY dependencies dependencies

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install software-properties-common && \
    DEBIAN_FRONTEND=noninteractive apt-add-repository -y "deb http://deb.debian.org/debian main contrib non-free" && \
    DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install $(dependencies/get_dependencies.sh build_base_debian_bookworm)

# Build recent autoconf from source
# RUN wget http://ftp.gnu.org/gnu/autoconf/autoconf-2.71.tar.gz && \
#     tar xvfz autoconf-2.71.tar.gz && \
#     cd autoconf-2.71 && \
#     ./configure --prefix=/usr/local && \
#     make -j$(nproc) && \
#     make install