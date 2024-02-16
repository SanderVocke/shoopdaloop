# syntax=docker/dockerfile:1
   
FROM quay.io/pypa/manylinux_2_28_x86_64:latest
WORKDIR /

# Python alias
RUN ln -s /usr/local/bin/python3.8 /usr/local/bin/python && \
    ln -s /usr/local/bin/python3.8 /usr/local/bin/python3 && \
    echo "export PATH=\"\$PATH:$(find /opt/_internal -name "cpython-3.8.*")/bin\"" >> $HOME/.bashrc

COPY dependencies dependencies

# System dependencies
RUN dnf config-manager --set-enabled powertools && \
    dny -y install epel-release && \
    dnf -y install $(dependencies/get_dependencies.sh base_manylinux_2_28)

# Build and install lcov
RUN curl -L https://github.com/linux-test-project/lcov/releases/download/v2.0/lcov-2.0.tar.gz --output lcov-2.0.tar.gz && \
    tar -xzf lcov-2.0.tar.gz && \
    pushd lcov-2.0 && \
    make install && \
    popd

# Python packages
RUN python3 -m pip install coverage pytest