# syntax=docker/dockerfile:1
   
FROM quay.io/pypa/manylinux_2_28_x86_64:latest
WORKDIR /

# Python alias
RUN ln -s /usr/local/bin/python3.8 /usr/local/bin/python && \
    ln -s /usr/local/bin/python3.8 /usr/local/bin/python3 && \
    echo "export PATH=\"\$PATH:$(find /opt/_internal -name "cpython-3.8.*")/bin\"" >> $HOME/.bashrc

COPY dependencies dependencies

RUN dnf -y install $(dependencies/get_dependencies.sh base_manylinux_2_28)
