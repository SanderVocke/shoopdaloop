# syntax=docker/dockerfile:1
   
FROM ubuntu:rolling
WORKDIR /

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get -y install sudo && \
    useradd -m build && \
    echo "build ALL=(ALL) NOPASSWD :ALL" > /etc/sudoers.d/10-build

USER build
WORKDIR /home/build
COPY dependencies dependencies

RUN sudo DEBIAN_FRONTEND=noninteractive apt-get -y install $(cat dependencies/ubuntu_run.txt | tr '\n' ' ') && \
    sudo DEBIAN_FRONTEND=noninteractive apt-get -y install $(cat dependencies/ubuntu_run_python.txt | tr '\n' ' ') && \
    sudo DEBIAN_FRONTEND=noninteractive apt-get -y install $(cat dependencies/ubuntu_build.txt | tr '\n' ' ') && \
    sudo DEBIAN_FRONTEND=noninteractive apt-get -y install $(cat dependencies/ubuntu_check.txt | tr '\n' ' ')

RUN sudo DEBIAN_FRONTEND=noninteractive apt-get -y install python3-pip && \
    sudo python3 -m pip install --break-system-packages $(cat dependencies/ubuntu_run_pip.txt | tr '\n' ' ') $(cat dependencies/ubuntu_build_pip.txt | tr '\n' ' ')