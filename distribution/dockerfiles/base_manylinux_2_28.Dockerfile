# syntax=docker/dockerfile:1
   
ARG architecture=x86_64
FROM quay.io/pypa/manylinux_2_28_${architecture}:latest
WORKDIR /

# Python alias
# (note: pre-installed python is a static build from manylinux)
RUN ln -s /usr/local/bin/python3.8 /usr/local/bin/python && \
    ln -s /usr/local/bin/python3.8 /usr/local/bin/python3 && \
    echo "export PATH=\"\$PATH:$(find /opt/_internal -name "cpython-3.8.*")/bin\"" >> $HOME/.bashrc

COPY dependencies dependencies

# System dependencies
RUN dnf config-manager --set-enabled powertools && \
    dnf -y install epel-release && \
    dnf -y install $(dependencies/get_dependencies.sh base_manylinux_2_28)

# Build and install lcov
RUN curl -L https://github.com/linux-test-project/lcov/releases/download/v2.0/lcov-2.0.tar.gz --output lcov-2.0.tar.gz && \
    tar -xzf lcov-2.0.tar.gz && \
    pushd lcov-2.0 && \
    make install && \
    popd

# FPM packaging tool
RUN dnf -y install autoconf gcc patch bzip2 openssl-devel libyaml-devel libffi-devel readline-devel zlib-devel gdbm-devel ncurses-devel tar && \
    curl -L https://github.com/rbenv/ruby-build/archive/refs/tags/v20240221.tar.gz --output rubybuild.tar.gz && \
    tar -xvf rubybuild.tar.gz && \
    export PREFIX=/usr/local && \
    ./ruby-build-*/install.sh && \
    ruby-build 3.3.0 /usr/local && \
    gem install fpm

# Python packages
RUN python3 -m pip install coverage pytest