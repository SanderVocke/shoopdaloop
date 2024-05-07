SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
WHICH=$1
BUILD_ARCH=${BUILD_ARCH:-"x86_64"}

cd ${SCRIPTPATH}/.. && podman build --build-arg architecture=${BUILD_ARCH} -f dockerfiles/$1.Dockerfile -t docker.io/sandervocke/shoopdaloop_$1:latest .